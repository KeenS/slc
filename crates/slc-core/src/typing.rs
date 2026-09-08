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
            Type::Bang(t) => Type::Bang(Box::new(self.apply(t))),
            Type::List(t) => Type::List(Box::new(self.apply(t))),
            Type::Fun(a, b) => Type::Fun(Box::new(self.apply(a)), Box::new(self.apply(b))),
            atom => atom.clone(),
        }
    }

    fn occurs(&self, var: usize, ty: &Type) -> bool {
        match ty {
            Type::Var(v) => {
                *v == var || self.substitutions.get(v).is_some_and(|t| self.occurs(var, t))
            }
            Type::Tensor(a, b)
            | Type::Par(a, b)
            | Type::With(a, b)
            | Type::Sum(a, b)
            | Type::Fun(a, b) => self.occurs(var, a) || self.occurs(var, b),
            Type::Dual(t) | Type::Bang(t) | Type::List(t) => self.occurs(var, t),
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
            (Type::Var(a), _) => {
                self.bind(*a, actual.clone())?;
                Ok(expected)
            }
            (_, Type::Var(b)) => {
                self.bind(*b, expected.clone())?;
                Ok(actual)
            }
            (Type::Dual(a), Type::Dual(b)) => self.unify(a, b),
            (Type::Bang(a), Type::Bang(b)) => self.unify(a, b),
            (Type::List(a), Type::List(b)) => self.unify(a, b),
            (Type::Fun(a1, a2), Type::Fun(b1, b2))
            | (Type::Tensor(a1, a2), Type::Tensor(b1, b2))
            | (Type::Par(a1, a2), Type::Par(b1, b2))
            | (Type::Sum(a1, a2), Type::Sum(b1, b2))
            | (Type::With(a1, a2), Type::With(b1, b2)) => {
                let left = self.unify(a1, b1)?;
                let right = self.unify(a2, b2)?;
                match (&expected, &actual) {
                    (Type::Fun(..), _) => Ok(Type::Fun(Box::new(left), Box::new(right))),
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

fn contains_var(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Tensor(a, b)
        | Type::Par(a, b)
        | Type::With(a, b)
        | Type::Sum(a, b)
        | Type::Fun(a, b) => contains_var(a) || contains_var(b),
        Type::Dual(t) | Type::Bang(t) | Type::List(t) => contains_var(t),
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
            Ok(Type::Fun(Box::new(xt), Box::new(bt)))
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

        Term::Inl(t) => {
            let a = infer_term(t, gamma, delta)?;
            Ok(Type::Sum(Box::new(a), Box::new(Type::Bottom)))
        }
        Term::Inr(t) => {
            let a = infer_term(t, gamma, delta)?;
            Ok(Type::Sum(Box::new(Type::Bottom), Box::new(a)))
        }
    }
}

/// Infer the type of a co-term: `Γ | e : A ⊢ Δ`
pub fn infer_coterm(
    e: &CoTerm,
    gamma: &mut TermContext,
    delta: &mut CoTermContext,
) -> Result<Type, TypeError> {
    match e {
        CoTerm::Covar(a) => delta.lookup(a).cloned().ok_or(TypeError::Unbound(a.clone())),

        CoTerm::CoLam(x, c) => {
            let xt =
                gamma.lookup(x).cloned().ok_or(TypeError::Unbound(format!("(co-param) {x}")))?;
            infer_command(c, gamma, delta)?;
            Ok(xt.dual())
        }

        CoTerm::MuTilde(x, c) => {
            let xt = gamma
                .lookup(x)
                .cloned()
                .ok_or(TypeError::Unbound(format!("(mutilde-param) {x}")))?;
            infer_command(c, gamma, delta)?;
            Ok(xt)
        }

        CoTerm::Par(e1, e2) => {
            let a = infer_coterm(e1, gamma, delta)?;
            let b = infer_coterm(e2, gamma, delta)?;
            Ok(Type::Par(Box::new(a), Box::new(b)))
        }

        CoTerm::Fst => Ok(Type::Fun(Box::new(Type::One), Box::new(Type::One))),
        CoTerm::Snd => Ok(Type::Fun(Box::new(Type::One), Box::new(Type::One))),
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
        Command::Command(x, t) => {
            let _ = infer_term(t, gamma, delta)?;
            let _ = x;
            Ok(())
        }
        Command::Activate(k, v) => {
            let kt = infer_term(k, gamma, delta)?;
            let vt = infer_term(v, gamma, delta)?;
            if kt.dual() == vt {
                Ok(())
            } else {
                Err(TypeError::Mismatch { expected: kt.dual(), actual: vt })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Base;

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
        let expected = Type::Fun(Box::new(a.clone()), Box::new(Type::Pos(Base::I32)));
        let actual = Type::Fun(Box::new(Type::Pos(Base::Bool)), Box::new(Type::Pos(Base::I32)));
        u.unify(&expected, &actual).unwrap();
        assert_eq!(u.apply(&a), Type::Pos(Base::Bool));
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
