//! λ̄μμ̃ terms (proofs).

use crate::command::Command;
use crate::coterm::CoTerm;

/// One branch of a menu (a negative additive value).
///
/// `label` is the destructor it answers — a fully qualified request name —
/// `binder` names the request's continuation inside `body`, and `body` is
/// the command that computes and delivers that answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoMatchBranch {
    pub label: String,
    pub binder: String,
    pub body: Box<Command>,
}

/// A term — the proof side of a cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// Variable reference.
    Var(String),
    /// λ abstraction: `λx.t`. A continuation is a value like any other, so a
    /// declared continuation parameter is an ordinary λ binder too — unlike
    /// `μα.c`, which captures the ambient continuation instead of receiving
    /// one from the caller.
    Lam(String, Box<Term>),
    /// μ abstraction: `μα.c`.
    Mu(String, Box<Command>),
    /// Tensor pair: `t1 ⊗ t2`.
    Pair(Box<Term>, Box<Term>),
    /// Labelled additive injection: `L(t)`. An `enum` value is a labelled
    /// injection; the label is the fully qualified
    /// variant name and the argument is the variant payload.
    Tag(String, Box<Term>),
    /// Menu: `μ[.d₁(α). c₁ | … | .dₙ(α). cₙ]` — the negative additive value,
    /// dual of [`CoTerm::CoCase`]. One branch per destructor the type
    /// offers; the request that arrives chooses exactly one of them, and
    /// the others are never evaluated. A `menu` value is a menu term.
    CoMatch(Vec<CoMatchBranch>),
    /// A co-term reified as a value: `co(e)` — the introduction form of the
    /// `↓` shift, boxing a consumer as data.
    ///
    /// The surface language lets a continuation appear where an expression is
    /// expected — `select` denotes one, and continuations are passed as
    /// arguments. `co(e)` is that continuation seen as a value: applying it
    /// — `⟨co(e) ∥ v · e′⟩` — sends the argument to the underlying co-term,
    /// which is the `↑` elimination opening the box.
    Co(Box<CoTerm>),
}
