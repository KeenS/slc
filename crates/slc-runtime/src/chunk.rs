//! The flat instruction stream the machine runs.
//!
//! The closed IR was a tree of `Rc`-linked nodes; walking it chased a pointer
//! and bumped a refcount at every child. Here the whole program is compiled
//! once into a single `Chunk` — a flat `Vec<Node>` — and every reference to a
//! sub-expression is a `NodeId`, an index into that vector. The machine holds
//! an instruction pointer (a `NodeId`) rather than an owned subtree, so
//! stepping into a child is an integer, not an allocation.
//!
//! One `Node` enum carries the term, co-term, and command forms together; the
//! machine knows which it expects from context (it is evaluating a term, or
//! consuming with a co-term, or running a command), exactly as the calculus
//! does, and the compiler only ever stores the right sort in each position.
//! Lexical variables are de Bruijn indices into the positional environment;
//! globals, literals, and `match` pattern variables stay names.

use std::cell::RefCell;
use std::rc::Rc;

/// An index into a `Chunk`'s node vector — the machine's instruction pointer.
pub type NodeId = u32;

/// One node of the flat program. Term, co-term, and command forms share the
/// vector; position determines which sort is expected.
#[derive(Debug, Clone)]
pub enum Node {
    // ── terms ──
    /// A lexical variable, by de Bruijn index (0 = innermost).
    Local(usize),
    /// A name: a literal encoding, a global, or an injected pattern variable.
    Dynamic(Rc<str>),
    /// `λ. t` — binds one positional slot; child is the body term.
    Lam(NodeId),
    /// `μ. c` — binds one positional slot; child is the command.
    Mu(NodeId),
    /// A tuple: its component terms, evaluated left to right.
    Tuple(Rc<Vec<NodeId>>),
    Tag(Rc<str>, NodeId),
    /// `μ[…]` — a menu value: its branches, each binding one positional
    /// slot (the request's continuation) before its body.
    CoMatch(Rc<Vec<Branch>>),
    /// A reified co-term value; child is the co-term node.
    Co(NodeId),

    // ── co-terms ──
    /// A lexical co-variable, by de Bruijn index.
    CoLocal(usize),
    /// A name used as a co-variable (a forwarding continuation, or a global).
    CoDynamic(Rc<str>),
    /// `v · e` — application: the argument term, then the tail co-term that
    /// consumes the result.
    App(NodeId, NodeId),
    /// `μ̃. c` — binds one positional slot; child is the command.
    MuTilde(NodeId),
    /// Projection of a tuple's `index`-th component.
    Prj(usize),
    /// A labelled consumer: its branches.
    CoCase(Rc<Vec<Branch>>),
    /// A product consumer binding `arity` slots; child is the command.
    MuTildeTensor(usize, NodeId),
    /// `.d(e)` — a request: the destructor label, and the co-term that
    /// consumes the answer.
    Dtor(Rc<str>, NodeId),

    // ── commands ──
    /// `⟨ t ∥ e ⟩` — a cut of a term against a co-term.
    Cut(NodeId, NodeId),
}

/// One branch of a compiled labelled consumer.
#[derive(Debug, Clone)]
pub struct Branch {
    pub label: Rc<str>,
    /// How many positional slots this branch binds before its body.
    pub arity: usize,
    /// The branch body — a command node.
    pub body: NodeId,
}

/// A flat program: its nodes, grown by the compiler and indexed by the
/// machine.
#[derive(Debug, Default)]
pub struct Chunk {
    nodes: Vec<Node>,
}

impl Chunk {
    pub fn new() -> Self {
        Chunk { nodes: Vec::new() }
    }

    /// Append a node and return its id.
    pub fn push(&mut self, node: Node) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(node);
        id
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }
}

thread_local! {
    /// The chunk the machine is currently running. A program compiles to one
    /// chunk, set here for the duration of its run; every value (a closure, a
    /// consumer) holds `NodeId`s into it, so a value never outlives the chunk
    /// it indexes. Nested runs (a handler's `resume`) share it.
    static CURRENT: RefCell<Option<Rc<Chunk>>> = const { RefCell::new(None) };
}

/// Run `body` with `chunk` installed as the current chunk, restoring whatever
/// was installed before (so nested compiles — a unit test inside a run —
/// leave the outer run's chunk intact).
pub fn with_chunk<R>(chunk: Rc<Chunk>, body: impl FnOnce() -> R) -> R {
    let previous = CURRENT.with(|c| c.borrow_mut().replace(chunk));
    let result = body();
    CURRENT.with(|c| *c.borrow_mut() = previous);
    result
}

/// Read node `id` of the current chunk. Panics only if the machine is run
/// with no chunk installed, which the entry points prevent.
pub fn node(id: NodeId) -> Node {
    CURRENT.with(|c| c.borrow().as_ref().expect("a chunk must be installed").node(id).clone())
}
