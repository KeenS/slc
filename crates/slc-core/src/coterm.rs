//! λ̄μμ̃ co-terms (refutations).

use crate::command::Command;
use crate::term::Term;

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
    /// Application: `v · e` — the canonical call stack. Consumes a function:
    /// `v` is the argument and `e` consumes the result, so `⟨f ∥ v · e⟩`
    /// applies `f` to `v` and sends what comes back on to `e`.
    App(Term, Box<CoTerm>),
    /// Value abstraction: `μ̃x.c`.
    MuTilde(String, Box<Command>),
    /// Projection of the `index`-th component of a right-nested product,
    /// counted along the spine (the last component is stored bare).
    Prj(usize),
    /// Labelled consumer: `μ̃[L₁(x…). c₁ | … | Lₙ(x…). cₙ]`.
    ///
    /// The dual of a labelled positive type: one branch per shape the value
    /// can take, and the label supplied at activation chooses exactly one of
    /// them. An `enum` has a branch per variant — the negative additive — and
    /// a `data` has exactly one, binding its fields.
    CoCase(Vec<CoCaseBranch>),
    /// Consumer of a product: `μ̃(x₁, …, xₙ). c` binds every component of the
    /// tensor it is given. This is the negative multiplicative: it is the
    /// unlabelled counterpart of a one-branch `CoCase`.
    MuTildeTensor(Vec<String>, Box<Command>),
    /// Destructor: `.d(e)` — a request, the dual of [`Term::Tag`]. The label
    /// selects one branch of a menu value, and the payload is the
    /// continuation that wants that branch's answer.
    Dtor(String, Box<CoTerm>),
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
