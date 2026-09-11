//! λ̄μμ̃ commands (cuts).

use crate::coterm::CoTerm;
use crate::term::Term;

/// A command — the interaction of a proof and a refutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Cut: `⟨ t ∥ e ⟩`.
    Cut(Term, CoTerm),
}
