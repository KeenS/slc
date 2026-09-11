//! Bidirectional type checking for λ̄μμ̃ sequents.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::term::Term;
use crate::types::Type;
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

/// A unification state for type variables.
#[derive(Debug, Clone, Default)]
pub struct Unification {
    /// Rigid variables: type parameters seen from inside their own body.
    /// They bind nothing and nothing binds them — `T` is whatever the caller
    /// chose, not a type the body may pick.
    rigid: std::collections::HashSet<usize>,
    substitutions: HashMap<usize, Type>,
    next_var: usize,
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

    pub fn apply(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(v) => match self.substitutions.get(v) {
                Some(t) => self.apply(t),
                None => ty.clone(),
            },
            Type::Tensor(a, b) => Type::Tensor(Box::new(self.apply(a)), Box::new(self.apply(b))),
            Type::Par(a, b) => Type::Par(Box::new(self.apply(a)), Box::new(self.apply(b))),
            Type::Dual(t) => Type::Dual(Box::new(self.apply(t))),
            Type::With(a, b) => Type::With(Box::new(self.apply(a)), Box::new(self.apply(b))),
            Type::Sum(a, b) => Type::Sum(Box::new(self.apply(a)), Box::new(self.apply(b))),
            Type::List(t) => Type::List(Box::new(self.apply(t))),
            Type::Down(t) => Type::Down(Box::new(self.apply(t))),
            Type::Up(t) => Type::Up(Box::new(self.apply(t))),
            atom => atom.clone(),
        }
    }

    fn occurs(&self, var: usize, ty: &Type) -> bool {
        match ty {
            Type::Var(v) => {
                *v == var || self.substitutions.get(v).is_some_and(|t| self.occurs(var, t))
            }
            Type::Tensor(a, b) | Type::Par(a, b) | Type::With(a, b) | Type::Sum(a, b) => {
                self.occurs(var, a) || self.occurs(var, b)
            }
            Type::Dual(t) | Type::List(t) | Type::Down(t) | Type::Up(t) => self.occurs(var, t),
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

    pub fn unify(&mut self, expected: &Type, actual: &Type) -> Result<Type, TypeError> {
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
            // `1` and `+unit` are one type written twice: `()` is the only
            // value of either.
            (Type::One, Type::Pos(crate::types::Base::Unit))
            | (Type::Pos(crate::types::Base::Unit), Type::One) => Ok(Type::One),
            (Type::Bottom, Type::Neg(crate::types::Base::Unit))
            | (Type::Neg(crate::types::Base::Unit), Type::Bottom) => Ok(Type::Bottom),
            (Type::Dual(a), Type::Dual(b)) => self.unify(a, b),
            // `dual` is semantic, not structural: `dual(X)` meets `B` when
            // `X` meets `dual(B)`.
            (Type::Dual(inner), other) | (other, Type::Dual(inner)) => {
                self.unify(inner, &other.dual())?;
                Ok(self.apply(&expected))
            }
            (Type::Down(a), Type::Down(b)) => Ok(Type::Down(Box::new(self.unify(a, b)?))),
            (Type::Up(a), Type::Up(b)) => Ok(Type::Up(Box::new(self.unify(a, b)?))),
            (Type::List(a), Type::List(b)) => self.unify(a, b),
            (Type::Tensor(a1, a2), Type::Tensor(b1, b2))
            | (Type::Par(a1, a2), Type::Par(b1, b2))
            | (Type::Sum(a1, a2), Type::Sum(b1, b2))
            | (Type::With(a1, a2), Type::With(b1, b2)) => {
                let left = self.unify(a1, b1)?;
                let right = self.unify(a2, b2)?;
                match (&expected, &actual) {
                    (Type::Tensor(..), _) => Ok(Type::Tensor(Box::new(left), Box::new(right))),
                    (Type::Par(..), _) => Ok(Type::Par(Box::new(left), Box::new(right))),
                    (Type::Sum(..), _) => Ok(Type::Sum(Box::new(left), Box::new(right))),
                    _ => Ok(Type::With(Box::new(left), Box::new(right))),
                }
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
        Type::Tensor(a, b) | Type::Par(a, b) | Type::With(a, b) | Type::Sum(a, b) => {
            contains_var(a) || contains_var(b)
        }
        Type::Down(t) | Type::Up(t) => contains_var(t),
        Type::Dual(t) | Type::List(t) => contains_var(t),
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

        Term::Pair(t1, t2) => {
            let a = infer_term(t1, gamma, delta)?;
            let b = infer_term(t2, gamma, delta)?;
            Ok(Type::Tensor(Box::new(a), Box::new(b)))
        }

        Term::Tag(label, payload) => {
            // A labelled injection belongs to the declaration that owns the
            // label: `Color::Red` inhabits the named positive type `Color`.
            let _ = infer_term(payload, gamma, delta)?;
            Ok(Type::Named(owner_of_label(label)?))
        }

        Term::CoMatch(branches) => {
            // A menu value inhabits the named negative type its destructors
            // belong to. Every branch must belong to the same declaration.
            let Some(first) = branches.first() else {
                return Err(TypeError::Arity("empty menu value".into()));
            };
            let owner = owner_of_label(&first.label)?;
            for branch in branches {
                if owner_of_label(&branch.label)? != owner {
                    return Err(TypeError::Arity(format!(
                        "menu value mixes `{owner}` with `{}`",
                        branch.label
                    )));
                }
                // The request's continuation scopes over the branch body only.
                let shadowed = delta.lookup(&branch.binder).cloned();
                delta.insert(branch.binder.clone(), Type::Bottom);
                let result = infer_command(&branch.body, gamma, delta);
                match shadowed {
                    Some(ty) => delta.insert(branch.binder.clone(), ty),
                    None => delta.remove(&branch.binder),
                }
                result?;
            }
            Ok(Type::Named(owner))
        }

        Term::Co(e) => {
            // A reified co-term is a value of the dual of what it refutes.
            Ok(infer_coterm(e, gamma, delta)?.dual())
        }
    }
}

/// The declaration a fully qualified variant label belongs to.
fn owner_of_label(label: &str) -> Result<String, TypeError> {
    label
        .split_once("::")
        .map(|(owner, _)| owner.to_string())
        .ok_or_else(|| TypeError::Arity(format!("label `{label}` is not `Type::Variant`")))
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
            // the stack consumes `A → B`.
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

        CoTerm::CoCase(branches) => {
            // A negative additive consumer refutes the named type its labels
            // belong to. Every branch must belong to the same declaration.
            let Some(first) = branches.first() else {
                return Err(TypeError::Arity("empty negative additive consumer".into()));
            };
            let owner = owner_of_label(&first.label)?;
            for branch in branches {
                if owner_of_label(&branch.label)? != owner {
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
                    gamma.insert(binder.clone(), Type::One);
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
            Ok(Type::Named(owner))
        }

        CoTerm::Dtor(label, e) => {
            // A request refutes the named negative type that owns its
            // destructor; the payload is the continuation for the answer.
            infer_coterm(e, gamma, delta)?;
            Ok(Type::Named(owner_of_label(label)?))
        }

        CoTerm::MuTildeTensor(binders, body) => {
            // A consumer of a product refutes the tensor of its components.
            let shadowed: Vec<_> = binders
                .iter()
                .map(|binder| (binder.clone(), gamma.lookup(binder).cloned()))
                .collect();
            for binder in binders {
                gamma.insert(binder.clone(), Type::One);
            }
            let result = infer_command(body, gamma, delta);
            for (binder, previous) in shadowed {
                match previous {
                    Some(ty) => gamma.insert(binder, ty),
                    None => gamma.remove(&binder),
                }
            }
            result?;
            Ok(binders
                .iter()
                .map(|_| Type::One)
                .reduce(|acc, ty| Type::Tensor(Box::new(acc), Box::new(ty)))
                .unwrap_or(Type::One))
        }

        CoTerm::Prj(_) => Ok(Type::arrow(Type::One, Type::One)),
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
    use crate::coterm::CoCaseBranch;
    use crate::types::Base;

    #[test]
    fn labelled_injection_has_its_declaration_type() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("v".into(), Type::One);
        let value = Term::Tag("Color::Red".into(), Box::new(Term::Var("v".into())));
        assert_eq!(infer_term(&value, &mut g, &mut d), Ok(Type::Named("Color".into())));
    }

    #[test]
    fn negative_additive_consumer_refutes_its_declaration_type() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        d.insert("k".into(), Type::Neg(Base::I32));
        g.insert("n".into(), Type::Pos(Base::I32));
        let consumer = CoTerm::CoCase(vec![CoCaseBranch {
            label: "Color::Red".into(),
            binders: vec!["x".into()],
            body: Box::new(Command::Cut(Term::Var("n".into()), CoTerm::Covar("k".into()))),
        }]);
        assert_eq!(infer_coterm(&consumer, &mut g, &mut d), Ok(Type::Named("Color".into())));
        // Reified as a value, it is dual to what it refutes — the type the
        // surface checker gives a `select` expression.
        let reified = Term::Co(Box::new(consumer));
        assert_eq!(
            infer_term(&reified, &mut g, &mut d),
            Ok(Type::Dual(Box::new(Type::Named("Color".into()))))
        );
    }

    #[test]
    fn negative_additive_consumer_rejects_mixed_declarations() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), Type::One);
        let mixed = CoTerm::CoCase(vec![
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
        ]);
        d.insert("k".into(), Type::Bottom);
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
        let expected = Type::Named("Point".into());
        let actual = Type::Named("Point".into());
        assert_eq!(u.unify(&expected, &actual).unwrap(), Type::Named("Point".into()));

        let actual = Type::Named("Color".into());
        assert!(matches!(
            u.unify(&Type::Named("Point".into()), &actual),
            Err(TypeError::Mismatch { .. })
        ));
        assert!(matches!(
            u.unify(&Type::Named("Point".into()), &Type::One),
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
        g.insert("y".into(), Type::Pos(Base::Bool));
        let t = Term::Pair(Box::new(Term::Var("x".into())), Box::new(Term::Var("y".into())));
        let ty = infer_term(&t, &mut g, &mut d).unwrap();
        assert_eq!(ty, Type::Tensor(Box::new(pos_i32()), Box::new(Type::Pos(Base::Bool))));
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
        assert_eq!(u.unify(&b, &Type::Pos(Base::Bool)).unwrap(), b);
        assert_eq!(u.apply(&a), Type::Pos(Base::I32));
        assert_eq!(u.apply(&b), Type::Pos(Base::Bool));
    }

    #[test]
    fn unification_structural() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        let expected = Type::arrow(a.clone(), Type::Pos(Base::I32));
        let actual = Type::arrow(Type::Pos(Base::Bool), Type::Pos(Base::I32));
        u.unify(&expected, &actual).unwrap();
        // `A → B` is `-A ⅋ B`, and `dual` is semantic: the wrapped variable
        // meets `-bool` by becoming `+bool` — the argument itself.
        assert_eq!(u.apply(&a), Type::Pos(Base::Bool));
    }

    #[test]
    fn the_two_spellings_of_unit_are_one_type() {
        // `()` is `1`, and the written type `unit` is `+unit`; a value of
        // one is a value of the other.
        let mut u = Unification::new();
        assert!(u.unify(&Type::One, &Type::Pos(Base::Unit)).is_ok());
        assert!(u.unify(&Type::Neg(Base::Unit), &Type::Bottom).is_ok());
    }

    #[test]
    fn occurs_check_rejects_recursive_type() {
        let mut u = Unification::new();
        let a = u.fresh_var();
        let recursive = Type::List(Box::new(a.clone()));
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
        d.insert("k".into(), Type::Neg(Base::Bool));
        let c = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        assert!(matches!(infer_command(&c, &mut g, &mut d), Err(TypeError::Mismatch { .. })));
    }
}
