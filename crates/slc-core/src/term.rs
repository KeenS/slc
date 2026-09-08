//! λ̄μμ̃ terms (proofs).

use crate::command::Command;

/// A term — the proof side of a cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// Variable reference.
    Var(String),
    /// λ abstraction: `λx.t`.
    Lam(String, Box<Term>),
    /// μ abstraction: `μα.c`.
    Mu(String, Box<Command>),
    /// Tensor pair: `t1 ⊗ t2`.
    Pair(Box<Term>, Box<Term>),
    /// Left additive injection.
    Inl(Box<Term>),
    /// Right additive injection.
    Inr(Box<Term>),
}
