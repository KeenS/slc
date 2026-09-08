//! λ̄μμ̃ co-terms (refutations).

use crate::command::Command;

/// A co-term — the refutation side of a cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoTerm {
    /// Co-variable reference.
    Covar(String),
    /// Co-abstraction: `λ̄x.c`.
    CoLam(String, Box<Command>),
    /// Value abstraction: `μ̃x.c`.
    MuTilde(String, Box<Command>),
    /// Par: `e1 ⅋ e2`.
    Par(Box<CoTerm>, Box<CoTerm>),
    /// First projection.
    Fst,
    /// Second projection.
    Snd,
}
