//! Bidirectional type checking for λ̄μμ̃ sequents.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::term::Term;
use crate::types::{Effect, Row, Type};
use std::collections::HashMap;

/// Term context: `Γ`
#[derive(Debug, Clone, Default)]
pub struct TermContext {
    pub vars: HashMap<String, Type>,
}

impl TermContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, x: String, t: Type) {
        self.vars.insert(x, t);
    }

    pub fn lookup(&self, x: &str) -> Option<&Type> {
        self.vars.get(x)
    }

    pub fn remove(&mut self, x: &str) {
        self.vars.remove(x);
    }
}

/// Co-term context: `Δ`
#[derive(Debug, Clone, Default)]
pub struct CoTermContext {
    pub covars: HashMap<String, Type>,
}

impl CoTermContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, a: String, t: Type) {
        self.covars.insert(a, t);
    }

    pub fn lookup(&self, a: &str) -> Option<&Type> {
        self.covars.get(a)
    }

    pub fn remove(&mut self, a: &str) {
        self.covars.remove(a);
    }
}

/// Error type for type checking.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeError {
    Unbound(String),
    Mismatch { expected: Type, actual: Type },
    Polarity { expected: &'static str, actual: Type },
    Arity(String),
    CannotInfer(String),
    Occurs(String),
    Recursive(String),
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeError::Unbound(x) => write!(f, "unbound variable: {x}"),
            TypeError::Mismatch { expected, actual } => {
                write!(f, "type mismatch: expected {expected}, got {actual}")
            }
            TypeError::Polarity { expected, actual } => {
                write!(f, "polarity mismatch: expected {expected}, got {actual}")
            }
            TypeError::Arity(msg) => write!(f, "arity error: {msg}"),
            TypeError::CannotInfer(msg) => write!(f, "cannot infer type: {msg}"),
            TypeError::Occurs(msg) => write!(f, "occurs check failed: {msg}"),
            TypeError::Recursive(msg) => write!(f, "recursive type: {msg}"),
        }
    }
}

/// A unification state for type variables, and the row constraints met on
/// the way.
#[derive(Debug, Clone, Default)]
pub struct Unification {
    /// Rigid variables: type parameters seen from inside their own body.
    /// They bind nothing and nothing binds them — `T` is whatever the caller
    /// chose, not a type the body may pick.
    rigid: std::collections::HashSet<usize>,
    substitutions: HashMap<usize, Type>,
    next_var: usize,
    /// Rigid row variables: a declaration's own `E`, seen from its body.
    rigid_rows: std::collections::HashSet<usize>,
    next_row: usize,
    /// Every "this row fits inside that one" recorded so far, in order.
    row_constraints: Vec<RowConstraint>,
    /// The negative declarations — menus and forms — whose bare name is the
    /// positive type of their demands, and whose dual is the value.
    negative_decls: std::collections::HashSet<String>,
    latent_decls: HashMap<String, (Row, Option<usize>)>,
}

/// One row fitting inside another: what a value performs, inside what its
/// slot allows (`docs/design-notes/rows-in-types.md`, "Subeffecting").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowConstraint {
    pub sub: Row,
    pub sup: Row,
}

/// What a solved row holds: an effect, or a rigid row variable standing for
/// whatever its declaration's caller chose.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowAtom {
    Effect(Effect),
    Rigid(usize),
}

/// A constraint no choice of rows satisfies: the atom its smaller side holds
/// that its larger side does not allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowFailure {
    /// The position of the constraint, in the order it was recorded.
    pub constraint: usize,
    pub atom: RowAtom,
}

impl Unification {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh_var(&mut self) -> Type {
        let v = self.next_var;
        self.next_var += 1;
        Type::Var(v)
    }

    /// Whether a variable is rigid: a type parameter seen from inside its
    /// own body, which nothing may generalize or bind.
    pub fn is_rigid(&self, var: usize) -> bool {
        self.rigid.contains(&var)
    }

    /// A rigid variable: a type parameter, seen from inside its own body.
    pub fn fresh_rigid(&mut self) -> Type {
        let v = self.next_var;
        self.next_var += 1;
        self.rigid.insert(v);
        Type::Var(v)
    }

    /// A flexible row variable: what some body performs, solved from below.
    pub fn fresh_row(&mut self) -> usize {
        let v = self.next_row;
        self.next_row += 1;
        v
    }

    /// A rigid row variable: a declaration's row parameter, from its body.
    pub fn fresh_rigid_row(&mut self) -> usize {
        let v = self.fresh_row();
        self.rigid_rows.insert(v);
        v
    }

    pub fn is_rigid_row(&self, var: usize) -> bool {
        self.rigid_rows.contains(&var)
    }

    /// Name the negative declarations, so a row argument on one is fitted
    /// the way its position asks (see `unify_in`).
    pub fn set_negative_decls(&mut self, names: impl IntoIterator<Item = String>) {
        self.negative_decls = names.into_iter().collect();
    }

    /// Record that `sub` fits inside `sup`. Nothing is checked until the
    /// constraints are solved.
    pub fn constrain_row(&mut self, sub: Row, sup: Row) {
        if !sub.is_empty() {
            self.row_constraints.push(RowConstraint { sub, sup });
        }
    }

    /// The row constraints recorded so far, in order.
    pub fn row_constraints(&self) -> &[RowConstraint] {
        &self.row_constraints
    }

    pub fn infer_row_arguments(&mut self, from: usize) {
        loop {
            let constraints = self.row_constraints[from..].to_vec();
            let failures = self.solve_rows(&constraints);
            let mut changed = false;
            for failure in failures {
                let RowAtom::Effect(actual) = failure.atom else { continue };
                for expected in &constraints[failure.constraint].sup.effects {
                    if expected.name != actual.name || expected.args.len() != actual.args.len() {
                        continue;
                    }
                    let mut trial = self.clone();
                    let before = trial.row_constraints.len();
                    if expected.args.iter().zip(&actual.args).all(|(expected, actual)| {
                        trial.unify_in(expected, actual, true, false).is_ok()
                    }) {
                        let added = trial.row_constraints.split_off(before);
                        for constraint in added {
                            if !trial.row_constraints.contains(&constraint) {
                                trial.row_constraints.push(constraint);
                            }
                        }
                        if trial.substitutions != self.substitutions
                            || trial.row_constraints.len() != before
                        {
                            *self = trial;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn effects_equated(
        &self,
        expected: &Effect,
        actual: &Effect,
        constraints: &[RowConstraint],
    ) -> bool {
        if expected.name != actual.name || expected.args.len() != actual.args.len() {
            return false;
        }
        if expected == actual {
            return true;
        }
        let mut trial = self.clone();
        let before = trial.row_constraints.len();
        expected
            .args
            .iter()
            .zip(&actual.args)
            .all(|(expected, actual)| trial.unify_in(expected, actual, true, false).is_ok())
            && trial.substitutions == self.substitutions
            && trial.row_constraints[before..]
                .iter()
                .all(|constraint| constraints.contains(constraint))
    }

    /// Solve `constraints`: every flexible row variable takes the least row
    /// its lower bounds give it, and each constraint whose larger side is
    /// concrete or rigid is checked against that. What does not fit is
    /// returned, one failure per atom, in the constraints' order.
    pub fn solve_rows(&self, constraints: &[RowConstraint]) -> Vec<RowFailure> {
        let constraints: Vec<_> = constraints
            .iter()
            .map(|constraint| RowConstraint {
                sub: constraint.sub.map_types(|argument| self.apply(argument)),
                sup: constraint.sup.map_types(|argument| self.apply(argument)),
            })
            .collect();
        let mut solved: HashMap<usize, std::collections::BTreeSet<RowAtom>> = HashMap::new();
        let atoms = |row: &Row, solved: &HashMap<usize, std::collections::BTreeSet<RowAtom>>| {
            let mut out: std::collections::BTreeSet<RowAtom> =
                row.effects.iter().cloned().map(RowAtom::Effect).collect();
            match row.tail {
                Some(tail) if self.is_rigid_row(tail) => {
                    out.insert(RowAtom::Rigid(tail));
                }
                Some(tail) => out.extend(solved.get(&tail).into_iter().flatten().cloned()),
                None => {}
            }
            out
        };
        loop {
            let mut changed = false;
            for constraint in &constraints {
                let Some(tail) = constraint.sup.tail.filter(|t| !self.is_rigid_row(*t)) else {
                    continue;
                };
                let grown: Vec<RowAtom> = atoms(&constraint.sub, &solved)
                    .into_iter()
                    .filter(|atom| !matches!(atom, RowAtom::Effect(effect)
                        if constraint.sup.effects.iter().any(|expected| expected.name == effect.name)))
                    .collect();
                let entry = solved.entry(tail).or_default();
                for atom in grown {
                    changed |= entry.insert(atom);
                }
            }
            if !changed {
                break;
            }
        }
        let mut failures = Vec::new();
        for (index, constraint) in constraints.iter().enumerate() {
            let allowed = atoms(&constraint.sup, &solved);
            for atom in atoms(&constraint.sub, &solved) {
                let accepted = match &atom {
                    RowAtom::Effect(actual) => {
                        match constraint
                            .sup
                            .effects
                            .iter()
                            .find(|expected| expected.name == actual.name)
                        {
                            Some(expected) => self.effects_equated(expected, actual, &constraints),
                            None => allowed.contains(&atom),
                        }
                    }
                    RowAtom::Rigid(_) => allowed.contains(&atom),
                };
                if !accepted {
                    failures.push(RowFailure { constraint: index, atom });
                }
            }
        }
        failures
    }

    pub fn apply(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(v) => match self.substitutions.get(v) {
                Some(t) => self.apply(t),
                None => ty.clone(),
            },
            Type::Tensor(xs) => Type::Tensor(xs.iter().map(|x| self.apply(x)).collect()),
            Type::Par(xs) => Type::Par(xs.iter().map(|x| self.apply(x)).collect()),
            // Dual is an involution, so the substitution reduces it: once
            // `?a` is known to be `-i64`, `dual(?a)` *is* `+i64`, and only a
            // still-unknown inner keeps the wrapper.
            Type::Dual(t) => self.apply(t).dual(),
            Type::With(xs) => Type::With(xs.iter().map(|x| self.apply(x)).collect()),
            Type::Sum(xs) => Type::Sum(xs.iter().map(|x| self.apply(x)).collect()),
            Type::Named(name, args) => {
                Type::Named(name.clone(), args.iter().map(|a| self.apply(a)).collect())
            }
            Type::Rowed(t, row) => {
                Type::Rowed(Box::new(self.apply(t)), row.map_types(|arg| self.apply(arg)))
            }
            Type::Delayed(inner, row) => {
                Type::delayed(self.apply(inner), row.map_types(|arg| self.apply(arg)))
            }
            atom => atom.clone(),
        }
    }

    fn occurs(&self, var: usize, ty: &Type) -> bool {
        match ty {
            Type::Var(v) => {
                *v == var || self.substitutions.get(v).is_some_and(|t| self.occurs(var, t))
            }
            Type::Tensor(xs) | Type::Par(xs) | Type::With(xs) | Type::Sum(xs) => {
                xs.iter().any(|x| self.occurs(var, x))
            }
            Type::Dual(t) => self.occurs(var, t),
            Type::Rowed(inner, row) | Type::Delayed(inner, row) => {
                self.occurs(var, inner)
                    || row
                        .effects
                        .iter()
                        .flat_map(|effect| &effect.args)
                        .any(|argument| self.occurs(var, argument))
            }
            Type::Named(_, args) => args.iter().any(|a| self.occurs(var, a)),
            _ => false,
        }
    }

    fn bind(&mut self, var: usize, ty: Type) -> Result<(), TypeError> {
        if self.occurs(var, &ty) {
            return Err(TypeError::Occurs(format!("?{var} occurs in {ty}")));
        }
        self.substitutions.insert(var, ty);
        Ok(())
    }

    /// Unify what a slot expects with the value that meets it. Where both
    /// carry rows, the value's row must fit inside the slot's, which is
    /// recorded for the solver; a row nested inside a component must be the
    /// same row on both sides.
    pub fn unify(&mut self, expected: &Type, actual: &Type) -> Result<Type, TypeError> {
        self.unify_in(expected, actual, false, false)
    }

    pub fn set_latent_decls(
        &mut self,
        declarations: impl IntoIterator<Item = (String, Row, Option<usize>)>,
    ) {
        self.latent_decls = declarations
            .into_iter()
            .map(|(name, row, parameter)| (name, (row, parameter)))
            .collect();
    }

    pub fn latent_row(&mut self, ty: &Type) -> Row {
        let Type::Dual(inner) = self.apply(ty) else { return Row::default() };
        let Type::Named(name, arguments) = *inner else { return Row::default() };
        let Some((mut row, parameter)) = self.latent_decls.get(&name).cloned() else {
            return Row::default();
        };
        row = row.map_types(|argument| argument.instantiate(&arguments));
        if let Some(argument) = parameter.and_then(|index| arguments.get(index)) {
            let argument = self.apply(argument);
            let given = match argument {
                Type::Rowed(unit, given) if *unit == Type::ONE => given,
                Type::Var(variable) if !self.rigid.contains(&variable) => {
                    let fresh = Row { effects: Default::default(), tail: Some(self.fresh_row()) };
                    let _ = self.bind(variable, Type::rowed(Type::ONE, fresh.clone()));
                    fresh
                }
                _ => Row::default(),
            };
            row.effects.extend(given.effects);
            row.tail = given.tail;
        }
        row
    }

    fn fit_rows(&mut self, expected: &Row, actual: &Row, nested: bool) {
        self.constrain_row(actual.clone(), expected.clone());
        if nested {
            self.constrain_row(expected.clone(), actual.clone());
        }
    }

    /// `flipped` says the two types sit under an odd number of `dual`s: what
    /// was the value's side is the slot's there, which is what decides which
    /// way a row argument is fitted.
    fn unify_in(
        &mut self,
        expected: &Type,
        actual: &Type,
        nested: bool,
        flipped: bool,
    ) -> Result<Type, TypeError> {
        let expected = self.apply(expected);
        let actual = self.apply(actual);
        match (&expected, &actual) {
            (Type::Var(a), Type::Var(b)) if a == b => Ok(expected),
            (Type::Var(a), _) if !self.rigid.contains(a) => {
                self.bind(*a, actual.clone())?;
                Ok(expected)
            }
            (_, Type::Var(b)) if !self.rigid.contains(b) => {
                self.bind(*b, expected.clone())?;
                Ok(actual)
            }
            // A rigid variable stands for a type the caller chose; only
            // itself (handled above) or a flexible variable (handled above,
            // by binding the flexible one) can meet it.
            (Type::Var(_), _) | (_, Type::Var(_)) => {
                Err(TypeError::Mismatch { expected: expected.clone(), actual: actual.clone() })
            }
            // `(,)` and `+unit` are one type written twice: `(,)` is the only
            // value of either.
            (Type::Tensor(xs), Type::Pos(crate::types::Base::Unit))
            | (Type::Pos(crate::types::Base::Unit), Type::Tensor(xs))
                if xs.is_empty() =>
            {
                Ok(Type::ONE)
            }
            (Type::Par(xs), Type::Neg(crate::types::Base::Unit))
            | (Type::Neg(crate::types::Base::Unit), Type::Par(xs))
                if xs.is_empty() =>
            {
                Ok(Type::BOTTOM)
            }
            // A row is compared, not unified: the value's must fit inside the
            // slot's. A type without one performs nothing.
            (Type::Delayed(inner_a, row_a), Type::Delayed(inner_b, row_b)) => {
                self.unify_in(inner_a, inner_b, nested, flipped)?;
                self.fit_rows(row_a, row_b, nested);
                Ok(self.apply(&expected))
            }
            (Type::Delayed(inner, row), other) => {
                self.unify_in(inner, other, nested, flipped)?;
                self.fit_rows(row, &Row::default(), nested);
                Ok(self.apply(&expected))
            }
            (other, Type::Delayed(inner, row)) => {
                self.unify_in(other, inner, nested, flipped)?;
                self.fit_rows(&Row::default(), row, nested);
                Ok(self.apply(&expected))
            }
            (Type::Rowed(a, row_a), Type::Rowed(b, row_b)) => {
                let (row_a, row_b) = (row_a.clone(), row_b.clone());
                self.unify_in(a, b, nested, flipped)?;
                self.fit_rows(&row_a, &row_b, nested);
                Ok(self.apply(&expected))
            }
            (Type::Rowed(a, row_a), other) => {
                let row_a = row_a.clone();
                self.unify_in(a, other, nested, flipped)?;
                self.fit_rows(&row_a, &Row::default(), nested);
                Ok(self.apply(&expected))
            }
            (other, Type::Rowed(b, row_b)) => {
                let row_b = row_b.clone();
                self.unify_in(other, b, nested, flipped)?;
                let allowed = self.latent_row(other);
                self.fit_rows(&allowed, &row_b, false);
                Ok(self.apply(&expected))
            }
            (Type::Dual(a), Type::Dual(b)) => self.unify_in(a, b, nested, !flipped),
            // `dual` is semantic, not structural: `dual(X)` meets `B` when
            // `X` meets `dual(B)`. Each side keeps its side.
            (Type::Dual(inner), other) | (other, Type::Dual(inner)) => {
                let turned = other.dual();
                // A declared type's dual stays wrapped, so flipping it makes no
                // progress: `dual(X)` against `X` would ask the same question
                // again forever. A type and its dual have opposite polarities,
                // so unless a variable is waiting to take it, they do not meet.
                if matches!(turned, Type::Dual(_)) && !matches!(inner.as_ref(), Type::Var(_)) {
                    return Err(TypeError::Mismatch {
                        expected: expected.clone(),
                        actual: actual.clone(),
                    });
                }
                if matches!(expected, Type::Dual(_)) {
                    self.unify_in(inner, &turned, nested, !flipped)?;
                } else {
                    self.unify_in(&turned, inner, nested, !flipped)?;
                }
                Ok(self.apply(&expected))
            }
            (Type::Named(a, xs), Type::Named(b, ys)) if a == b && xs.len() == ys.len() => {
                if a == "Handler" {
                    let args = xs
                        .iter()
                        .zip(ys)
                        .map(|(expected, actual)| self.unify_in(expected, actual, true, flipped))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(Type::Named(a.clone(), args));
                }
                // A row argument, carried on the unit, says what running the
                // value performs: one that performs less fits where more is
                // allowed, so it is fitted one way, not made equal. Which way
                // is the position's. The bare name of a menu or form is its
                // demand, and the dual of a data or enum name its consumer:
                // there the slot's row must fit inside the value's, since the
                // value must take everything the slot could be given.
                let covariant = self.negative_decls.contains(a) == flipped;
                // The arguments sit at the value's side when the position is
                // covariant, whatever `dual`s stand around the name.
                let inside = !covariant;
                let args = xs
                    .iter()
                    .zip(ys)
                    .map(|(x, y)| {
                        let row_argument = [x, y].into_iter().any(
                            |t| matches!(self.apply(t), Type::Rowed(unit, _) if *unit == Type::ONE),
                        );
                        match (row_argument, covariant) {
                            (false, _) => self.unify_in(x, y, true, inside),
                            (true, true) => self.unify_in(x, y, false, inside),
                            (true, false) => self.unify_in(y, x, false, inside),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Named(a.clone(), args))
            }
            // The same connective over as many components: componentwise.
            // Nesting is significant, so a count that differs is a mismatch.
            (Type::Tensor(xs), Type::Tensor(ys))
            | (Type::Par(xs), Type::Par(ys))
            | (Type::Sum(xs), Type::Sum(ys))
            | (Type::With(xs), Type::With(ys))
                if xs.len() == ys.len() =>
            {
                let components = xs
                    .iter()
                    .zip(ys)
                    .map(|(x, y)| self.unify_in(x, y, true, flipped))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(match &expected {
                    Type::Tensor(_) => Type::Tensor(components),
                    Type::Par(_) => Type::Par(components),
                    Type::Sum(_) => Type::Sum(components),
                    _ => Type::With(components),
                })
            }
            (a, b) if a == b => Ok(a.clone()),
            (a, b) => Err(TypeError::Mismatch { expected: a.clone(), actual: b.clone() }),
        }
    }

    /// Unify two types and additionally require the result to have the
    /// requested polarity.
    pub fn unify_with_polarity(
        &mut self,
        expected: &Type,
        actual: &Type,
        positive: bool,
    ) -> Result<Type, TypeError> {
        let unified = self.unify(expected, actual)?;
        let unified = self.apply(&unified);
        if positive && !unified.is_positive() {
            return Err(TypeError::Polarity { expected: "positive", actual: unified });
        }
        if !positive && !unified.is_negative() {
            return Err(TypeError::Polarity { expected: "negative", actual: unified });
        }
        Ok(unified)
    }

    pub fn resolve_or_cannot_infer(&self, ty: &Type, context: &str) -> Result<Type, TypeError> {
        let resolved = self.apply(ty);
        if contains_var(&resolved) {
            return Err(TypeError::CannotInfer(format!("{context}: unresolved {resolved}")));
        }
        Ok(resolved)
    }
}

pub fn contains_var(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Tensor(xs) | Type::Par(xs) | Type::With(xs) | Type::Sum(xs) => {
            xs.iter().any(contains_var)
        }

        Type::Dual(t) => contains_var(t),
        Type::Rowed(inner, row) | Type::Delayed(inner, row) => {
            contains_var(inner)
                || row.effects.iter().flat_map(|effect| &effect.args).any(contains_var)
        }
        Type::Named(_, args) => args.iter().any(contains_var),
        _ => false,
    }
}

impl std::error::Error for TypeError {}

/// Infer the type of a term: `Γ ⊢ t : A | Δ`
pub fn infer_term(
    t: &Term,
    gamma: &mut TermContext,
    delta: &mut CoTermContext,
) -> Result<Type, TypeError> {
    match t {
        Term::Var(x) => gamma.lookup(x).cloned().ok_or(TypeError::Unbound(x.clone())),

        Term::Lam(x, body) => {
            // λx.t : needs x's type; bidirectional: assume check mode
            // For inference, we require x's type to be in the context already.
            let xt = gamma.lookup(x).cloned().ok_or(TypeError::Unbound(format!("(param) {x}")))?;
            let bt = infer_term(body, gamma, delta)?;
            Ok(Type::arrow(xt, bt))
        }

        Term::Mu(a, body) => {
            // μα.c : the answer type of command c
            let at = delta.lookup(a).cloned().ok_or(TypeError::Unbound(format!("(covar) {a}")))?;
            infer_command(body, gamma, delta)?;
            Ok(at.dual())
        }

        Term::Tuple(items) => Ok(Type::Tensor(
            items
                .iter()
                .map(|item| infer_term(item, gamma, delta))
                .collect::<Result<Vec<_>, _>>()?,
        )),

        Term::Tag(label, payload) => {
            // A labelled injection belongs to the declaration that owns the
            // label: `Color::Red` inhabits the named positive type `Color`.
            let _ = infer_term(payload, gamma, delta)?;
            Ok(Type::Named(owner_of_label(label), Vec::new()))
        }

        Term::CoMatch { owner, branches } => {
            // `(&)`: the empty menu is the unit of `&` itself, not a declaration.
            if owner == "(&)" && branches.is_empty() {
                return Ok(Type::TOP);
            }
            // A menu value inhabits the named negative type its destructors
            // belong to. Every branch must belong to the same declaration.
            for branch in branches {
                if owner_of_label(&branch.label) != owner.as_str() {
                    return Err(TypeError::Arity(format!(
                        "menu value mixes `{owner}` with `{}`",
                        branch.label
                    )));
                }
                // The request's continuation scopes over the branch body only.
                let shadowed = delta.lookup(&branch.binder).cloned();
                delta.insert(branch.binder.clone(), Type::BOTTOM);
                let result = infer_command(&branch.body, gamma, delta);
                match shadowed {
                    Some(ty) => delta.insert(branch.binder.clone(), ty),
                    None => delta.remove(&branch.binder),
                }
                result?;
            }
            Ok(Type::Named(owner.clone(), Vec::new()))
        }

        Term::Co(e) => {
            // A reified co-term is a value of the dual of what it refutes.
            Ok(infer_coterm(e, gamma, delta)?.dual())
        }
    }
}

/// The declaration a fully qualified variant label belongs to.
fn owner_of_label(label: &str) -> String {
    label.rsplit_once("::").map(|(owner, _)| owner.to_string()).unwrap_or_else(|| label.to_string())
}

/// Infer the type of a co-term: `Γ | e : A ⊢ Δ`
pub fn infer_coterm(
    e: &CoTerm,
    gamma: &mut TermContext,
    delta: &mut CoTermContext,
) -> Result<Type, TypeError> {
    match e {
        CoTerm::Covar(a) => delta.lookup(a).cloned().ok_or(TypeError::Unbound(a.clone())),

        CoTerm::App(v, e) => {
            // `v · e` refutes a function: with `v : A` and `e` refuting `B`,
            // the stack consumes `(A -> B)`.
            let vt = infer_term(v, gamma, delta)?;
            let et = infer_coterm(e, gamma, delta)?;
            Ok(Type::arrow(vt, et))
        }

        CoTerm::MuTilde(x, c) => {
            let xt = gamma
                .lookup(x)
                .cloned()
                .ok_or(TypeError::Unbound(format!("(mutilde-param) {x}")))?;
            infer_command(c, gamma, delta)?;
            Ok(xt)
        }

        CoTerm::CoCase { owner, branches } => {
            // `(|)`: the consumer with no arms refutes 0 itself.
            if owner == "(|)" && branches.is_empty() {
                return Ok(Type::ZERO);
            }
            // A negative additive consumer refutes the named type its labels
            // belong to. Every branch must belong to the same declaration.
            for branch in branches {
                if owner_of_label(&branch.label) != owner.as_str() {
                    return Err(TypeError::Arity(format!(
                        "negative additive consumer mixes `{owner}` with `{}`",
                        branch.label
                    )));
                }
                // The payload binders scope over the branch body only.
                let shadowed: Vec<_> = branch
                    .binders
                    .iter()
                    .map(|binder| (binder.clone(), gamma.lookup(binder).cloned()))
                    .collect();
                for binder in &branch.binders {
                    gamma.insert(binder.clone(), Type::ONE);
                }
                let result = infer_command(&branch.body, gamma, delta);
                for (binder, previous) in shadowed {
                    match previous {
                        Some(ty) => gamma.insert(binder, ty),
                        None => gamma.remove(&binder),
                    }
                }
                result?;
            }
            Ok(Type::Named(owner.clone(), Vec::new()))
        }

        CoTerm::Dtor(label, e) => {
            // A request refutes the named negative type that owns its
            // destructor; the payload is the continuation for the answer.
            infer_coterm(e, gamma, delta)?;
            Ok(Type::Named(owner_of_label(label), Vec::new()))
        }

        CoTerm::MuTildeTensor(binders, body) => {
            // A consumer of a product refutes the tensor of its components.
            let shadowed: Vec<_> = binders
                .iter()
                .map(|binder| (binder.clone(), gamma.lookup(binder).cloned()))
                .collect();
            for binder in binders {
                gamma.insert(binder.clone(), Type::ONE);
            }
            let result = infer_command(body, gamma, delta);
            for (binder, previous) in shadowed {
                match previous {
                    Some(ty) => gamma.insert(binder, ty),
                    None => gamma.remove(&binder),
                }
            }
            result?;
            Ok(match binders.len() {
                1 => Type::ONE,
                n => Type::Tensor(vec![Type::ONE; n]),
            })
        }

        CoTerm::Prj(_) => Ok(Type::arrow(Type::ONE, Type::ONE)),
    }
}

/// Check a command: cuts must have dual types.
pub fn infer_command(
    c: &Command,
    gamma: &mut TermContext,
    delta: &mut CoTermContext,
) -> Result<(), TypeError> {
    match c {
        Command::Cut(t, e) => {
            let tt = infer_term(t, gamma, delta)?;
            let et = infer_coterm(e, gamma, delta)?;
            if tt.dual() == et {
                Ok(())
            } else {
                Err(TypeError::Mismatch { expected: tt.dual(), actual: et })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_type_does_not_meet_its_own_dual() {
        // The dual of a declared type stays wrapped; unifying the two used to
        // recurse without end.
        let named = Type::Named("Color".into(), Vec::new());
        let mut u = Unification::new();
        assert!(matches!(
            u.unify(&Type::Dual(Box::new(named.clone())), &named),
            Err(TypeError::Mismatch { .. })
        ));
        assert!(matches!(
            u.unify(&named, &Type::Dual(Box::new(named.clone()))),
            Err(TypeError::Mismatch { .. })
        ));
        // A variable under the dual still takes the other side's dual.
        let v = u.fresh_var();
        u.unify(&Type::Dual(Box::new(v.clone())), &named).unwrap();
        assert_eq!(u.apply(&v), Type::Dual(Box::new(named)));
    }
    use crate::coterm::CoCaseBranch;
    use crate::types::Base;

    #[test]
    fn labelled_injection_has_its_declaration_type() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("v".into(), Type::ONE);
        let value = Term::Tag("Color::Red".into(), Box::new(Term::Var("v".into())));
        assert_eq!(infer_term(&value, &mut g, &mut d), Ok(Type::Named("Color".into(), Vec::new())));
    }

    #[test]
    fn empty_menu_retains_its_declaration_type() {
        let mut gamma = TermContext::new();
        let mut delta = CoTermContext::new();
        let top = Term::CoMatch { owner: "Top".into(), branches: Vec::new() };
        assert_eq!(
            infer_term(&top, &mut gamma, &mut delta),
            Ok(Type::Named("Top".into(), Vec::new()))
        );
    }

    #[test]
    fn empty_case_consumer_retains_its_declaration_type() {
        let mut gamma = TermContext::new();
        let mut delta = CoTermContext::new();
        let empty = CoTerm::CoCase { owner: "Empty".into(), branches: Vec::new() };
        assert_eq!(
            infer_coterm(&empty, &mut gamma, &mut delta),
            Ok(Type::Named("Empty".into(), Vec::new()))
        );
    }

    #[test]
    fn negative_additive_consumer_refutes_its_declaration_type() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        d.insert("k".into(), Type::Neg(Base::I32));
        g.insert("n".into(), Type::Pos(Base::I32));
        let consumer = CoTerm::CoCase {
            owner: "Color".into(),
            branches: vec![CoCaseBranch {
                label: "Color::Red".into(),
                binders: vec!["x".into()],
                body: Box::new(Command::Cut(Term::Var("n".into()), CoTerm::Covar("k".into()))),
            }],
        };
        assert_eq!(
            infer_coterm(&consumer, &mut g, &mut d),
            Ok(Type::Named("Color".into(), Vec::new()))
        );
        // Reified as a value, it is dual to what it refutes — the type the
        // surface checker gives a `select` expression.
        let reified = Term::Co(Box::new(consumer));
        assert_eq!(
            infer_term(&reified, &mut g, &mut d),
            Ok(Type::Dual(Box::new(Type::Named("Color".into(), Vec::new()))))
        );
    }

    #[test]
    fn negative_additive_consumer_rejects_mixed_declarations() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), Type::ONE);
        let mixed = CoTerm::CoCase {
            owner: "Color".into(),
            branches: vec![
                CoCaseBranch {
                    label: "Color::Red".into(),
                    binders: vec!["x".into()],
                    body: Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
                },
                CoCaseBranch {
                    label: "Shape::Circle".into(),
                    binders: vec!["x".into()],
                    body: Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
                },
            ],
        };
        d.insert("k".into(), Type::BOTTOM);
        assert!(matches!(infer_coterm(&mixed, &mut g, &mut d), Err(TypeError::Arity(_))));
    }

    fn pos_i32() -> Type {
        Type::Pos(Base::I32)
    }

    #[test]
    fn var_inference() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), pos_i32());
        let t = infer_term(&Term::Var("x".into()), &mut g, &mut d).unwrap();
        assert_eq!(t, pos_i32());
    }

    #[test]
    fn named_types_unify_by_name_and_reject_mismatches() {
        let mut u = Unification::new();
        let expected = Type::Named("Point".into(), Vec::new());
        let actual = Type::Named("Point".into(), Vec::new());
        assert_eq!(u.unify(&expected, &actual).unwrap(), Type::Named("Point".into(), Vec::new()));

        let actual = Type::Named("Color".into(), Vec::new());
        assert!(matches!(
            u.unify(&Type::Named("Point".into(), Vec::new()), &actual),
            Err(TypeError::Mismatch { .. })
        ));
        assert!(matches!(
            u.unify(&Type::Named("Point".into(), Vec::new()), &Type::ONE),
            Err(TypeError::Mismatch { .. })
        ));
    }

    #[test]
    fn unbound_var() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        let r = infer_term(&Term::Var("y".into()), &mut g, &mut d);
        assert!(matches!(r, Err(TypeError::Unbound(_))));
    }

    #[test]
    fn pair_inference() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), pos_i32());
        g.insert("y".into(), Type::Pos(Base::Char));
        let t = Term::Tuple(vec![Term::Var("x".into()), Term::Var("y".into())]);
        let ty = infer_term(&t, &mut g, &mut d).unwrap();
        assert_eq!(ty, Type::Tensor(vec![pos_i32(), Type::Pos(Base::Char)]));
    }

    #[test]
    fn cut_type_checks() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), pos_i32());
        d.insert("k".into(), pos_i32().dual());
        let c = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        assert!(infer_command(&c, &mut g, &mut d).is_ok());
    }

    #[test]
    fn unification_binds_variables() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        let b = u.fresh_var();
        assert_eq!(u.unify(&a, &Type::Pos(Base::I32)).unwrap(), a);
        assert_eq!(u.unify(&b, &Type::Pos(Base::Char)).unwrap(), b);
        assert_eq!(u.apply(&a), Type::Pos(Base::I32));
        assert_eq!(u.apply(&b), Type::Pos(Base::Char));
    }

    #[test]
    fn unification_structural() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        let expected = Type::arrow(a.clone(), Type::Pos(Base::I32));
        let actual = Type::arrow(Type::Pos(Base::Char), Type::Pos(Base::I32));
        u.unify(&expected, &actual).unwrap();
        // `A -> B` is `(dual(A) ; B)`, and `dual` is semantic: the wrapped variable
        // meets `-bool` by becoming `+bool` — the argument itself.
        assert_eq!(u.apply(&a), Type::Pos(Base::Char));
    }

    #[test]
    fn the_two_spellings_of_unit_are_one_type() {
        // `(,)` is the unit, and the written type `unit` is `+unit`; a value of
        // one is a value of the other.
        let mut u = Unification::new();
        assert!(u.unify(&Type::ONE, &Type::Pos(Base::Unit)).is_ok());
        assert!(u.unify(&Type::Neg(Base::Unit), &Type::BOTTOM).is_ok());
    }

    #[test]
    fn occurs_check_rejects_recursive_type() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        let recursive = Type::Named("List".into(), vec![a.clone()]);
        assert!(matches!(u.unify(&a, &recursive), Err(TypeError::Occurs(_))));
    }

    #[test]
    fn polarity_constrained_unification() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        assert_eq!(
            u.unify_with_polarity(&a, &Type::Pos(Base::I32), true).unwrap(),
            Type::Pos(Base::I32)
        );
        let b = u.fresh_var();
        assert!(matches!(
            u.unify_with_polarity(&b, &Type::Pos(Base::I32), false),
            Err(TypeError::Polarity { .. })
        ));
    }

    #[test]
    fn cannot_infer_unresolved_variable() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        assert!(matches!(
            u.resolve_or_cannot_infer(&a, "lambda parameter"),
            Err(TypeError::CannotInfer(_))
        ));
    }

    #[test]
    fn cut_type_mismatch() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), pos_i32());
        d.insert("k".into(), Type::Neg(Base::Char));
        let c = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        assert!(matches!(infer_command(&c, &mut g, &mut d), Err(TypeError::Mismatch { .. })));
    }
}
