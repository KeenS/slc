//! λ̄μμ̃ co-terms (refutations).

use crate::command::Command;

/// One branch of a labelled consumer.
///
/// `label` is the shape it accepts — a variant, or a struct's own name —
/// `binders` name that shape's components inside `body`, and `body` is the
/// command that runs when the consumer is activated with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoCaseBranch {
    pub label: String,
    pub binders: Vec<String>,
    pub body: Box<Command>,
}

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
    /// Labelled consumer: `μ̃[L₁(x…). c₁ | … | Lₙ(x…). cₙ]`.
    ///
    /// The dual of a labelled positive type: one branch per shape the value
    /// can take, and the label supplied at activation chooses exactly one of
    /// them. An `enum` has a branch per variant — the negative additive — and
    /// a `struct` has exactly one, binding its fields.
    CoCase(Vec<CoCaseBranch>),
    /// Consumer of a product: `μ̃(x₁, …, xₙ). c` binds every component of the
    /// tensor it is given. This is the negative multiplicative: it is the
    /// unlabelled counterpart of a one-branch `CoCase`.
    MuTildeTensor(Vec<String>, Box<Command>),
}

impl CoTerm {
    /// The branch a labelled value selects, if this is a negative additive
    /// consumer that covers the label.
    pub fn branch(&self, label: &str) -> Option<&CoCaseBranch> {
        match self {
            CoTerm::CoCase(branches) => branches.iter().find(|b| b.label == label),
            _ => None,
        }
    }
}
