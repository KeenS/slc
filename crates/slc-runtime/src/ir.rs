//! The closed intermediate representation the machine runs.
//!
//! Core λ̄μμ̃ names its (co-)variables; resolving a name meant a walk of the
//! environment comparing strings on every reference. The IR is the same
//! calculus with the lexical binders resolved once, at compile time, to de
//! Bruijn indices into a positional environment (`Local`/`CoLocal`). What a
//! compiler cannot resolve lexically — a global function, an enum
//! constructor, a literal, or a `match` arm's pattern variables, which are
//! injected by the pattern engine at run time — stays a name (`Dynamic`),
//! looked up in the named overlay and the globals table.
//!
//! Term variables and co-variables share one environment at run time (a
//! co-variable binds to a consumer value), so `Local` and `CoLocal` index
//! the same positional chain and the compiler counts every binder, of either
//! sort, in one scope.

use std::rc::Rc;

/// A term — the proof side, compiled.
#[derive(Debug, Clone)]
pub enum Ir {
    /// A lexical variable, by de Bruijn index into the positional chain
    /// (0 is the innermost binder).
    Local(usize),
    /// An unresolved name: a literal encoding, a global, or a pattern
    /// variable injected by the match engine.
    Dynamic(Rc<str>),
    /// `λ. t` — binds one positional slot (the parameter name is gone).
    Lam(Rc<Ir>),
    /// `μ. c` — binds one positional slot (the captured continuation).
    Mu(Rc<ICommand>),
    Pair(Rc<Ir>, Rc<Ir>),
    Inl(Rc<Ir>),
    Inr(Rc<Ir>),
    Tag(Rc<str>, Rc<Ir>),
    /// `Λ. t` — the negative function; binds one positional slot.
    CoAbs(Rc<Ir>),
    Co(Rc<ICoTerm>),
}

/// A co-term — the refutation side, compiled.
#[derive(Debug, Clone)]
pub enum ICoTerm {
    /// A lexical co-variable, by de Bruijn index into the positional chain.
    CoLocal(usize),
    /// An unresolved co-name (a continuation that only forwards onward, or a
    /// global consumer).
    CoDynamic(Rc<str>),
    /// `λ̄. c` — binds one positional slot.
    CoLam(Rc<ICommand>),
    /// `μ̃. c` — binds one positional slot.
    MuTilde(Rc<ICommand>),
    Par(Rc<ICoTerm>, Rc<ICoTerm>),
    Fst,
    Snd,
    /// A labelled consumer: each branch binds its arity's worth of slots.
    CoCase(Vec<IBranch>),
    /// A product consumer: binds `arity` positional slots.
    MuTildeTensor(usize, Rc<ICommand>),
}

/// One branch of a compiled labelled consumer.
#[derive(Debug, Clone)]
pub struct IBranch {
    pub label: Rc<str>,
    /// How many positional slots this branch binds before its body.
    pub arity: usize,
    pub body: Rc<ICommand>,
}

/// A command — a cut, compiled.
#[derive(Debug, Clone)]
pub enum ICommand {
    Cut(Rc<Ir>, Rc<ICoTerm>),
    /// `κx. t` binds nothing at run time; only the term matters.
    Command(Rc<Ir>),
    Activate(Rc<Ir>, Rc<Ir>),
}
