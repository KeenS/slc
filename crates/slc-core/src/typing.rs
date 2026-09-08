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
        }
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
    fn cut_type_mismatch() {
        let mut g = TermContext::new();
        let mut d = CoTermContext::new();
        g.insert("x".into(), pos_i32());
        d.insert("k".into(), Type::Neg(Base::Bool));
        let c = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        assert!(matches!(infer_command(&c, &mut g, &mut d), Err(TypeError::Mismatch { .. })));
    }
}
