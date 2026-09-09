//! λ̄μμ̃ terms (proofs).

use crate::command::Command;
use crate::coterm::CoTerm;

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
    /// Labelled additive injection: `L(t)`. An `enum` value is the labelled
    /// generalization of `inl`/`inr`; the label is the fully qualified
    /// variant name and the argument is the variant payload.
    Tag(String, Box<Term>),
    /// Continuation abstraction: `Λα. t`.
    ///
    /// This is the negative function: it abstracts a *declared* continuation
    /// parameter, so the continuation is supplied by the caller. It is not
    /// `μα.c`, which captures the ambient continuation instead.
    CoAbs(String, Box<Term>),
    /// A co-term reified as a negative value: `co(e)`.
    ///
    /// The surface language lets a continuation appear where an expression is
    /// expected — `select` denotes one, and continuations are passed as
    /// arguments. `co(e)` is that continuation seen as a value: cutting it
    /// against a consumer applies the consumer to the underlying co-term.
    Co(Box<CoTerm>),
}
