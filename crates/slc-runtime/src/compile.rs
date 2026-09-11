//! Compile core λ̄μμ̃ to the flat `Chunk`: resolve each lexical (co-)variable
//! to a de Bruijn index, and lay every node out in one vector.
//!
//! The compiler carries a scope — the binders in force, innermost last, term
//! and co-variables together, since they share one run-time environment. A
//! reference resolves to `Local`/`CoLocal` when its name is in scope, and to
//! `Dynamic`/`CoDynamic` otherwise (a literal, a global, or a pattern
//! variable the match engine injects at run time). Each node is appended to
//! the chunk and referred to by its id.

use crate::chunk::{Branch, Chunk, Node, NodeId};
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::term::Term;
use std::rc::Rc;

/// The binders in force while compiling, innermost last.
#[derive(Default, Clone)]
struct Scope {
    names: Vec<String>,
}

impl Scope {
    fn with(&self, extra: &[String]) -> Scope {
        let mut names = self.names.clone();
        names.extend_from_slice(extra);
        Scope { names }
    }

    /// The de Bruijn index of `name` (0 = innermost), if bound.
    fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().rposition(|n| n == name).map(|pos| self.names.len() - 1 - pos)
    }
}

/// Compile a single core term into its own chunk, returning the chunk and the
/// root node.
pub(crate) fn compile_term(t: &Term) -> (Rc<Chunk>, NodeId) {
    let mut chunk = Chunk::new();
    let root = compile_ir(t, &Scope::default(), &mut chunk);
    (Rc::new(chunk), root)
}

/// Compile a single core command into its own chunk.
pub(crate) fn compile_command(c: &Command) -> (Rc<Chunk>, NodeId) {
    let mut chunk = Chunk::new();
    let root = compile_cmd(c, &Scope::default(), &mut chunk);
    (Rc::new(chunk), root)
}

/// Compile every top-level definition into one shared chunk, returning it and
/// each definition's root node. A closure built from any of them indexes this
/// chunk, so they must share it.
pub fn compile_program(defs: &[(String, Term)]) -> (Rc<Chunk>, Vec<(String, NodeId)>) {
    let mut chunk = Chunk::new();
    let roots = defs
        .iter()
        .map(|(name, term)| (name.clone(), compile_ir(term, &Scope::default(), &mut chunk)))
        .collect();
    (Rc::new(chunk), roots)
}

fn compile_ir(t: &Term, scope: &Scope, chunk: &mut Chunk) -> NodeId {
    let node = match t {
        Term::Var(x) => match scope.index(x) {
            Some(i) => Node::Local(i),
            None => Node::Dynamic(Rc::from(x.as_str())),
        },
        Term::Lam(x, body) => {
            let body = compile_ir(body, &scope.with(std::slice::from_ref(x)), chunk);
            Node::Lam(body)
        }
        Term::Mu(a, c) => {
            let body = compile_cmd(c, &scope.with(std::slice::from_ref(a)), chunk);
            Node::Mu(body)
        }
        Term::Pair(a, b) => {
            let a = compile_ir(a, scope, chunk);
            let b = compile_ir(b, scope, chunk);
            Node::Pair(a, b)
        }
        Term::Tag(label, payload) => {
            Node::Tag(Rc::from(label.as_str()), compile_ir(payload, scope, chunk))
        }
        Term::CoMatch(branches) => {
            let branches = branches
                .iter()
                .map(|b| {
                    let body =
                        compile_cmd(&b.body, &scope.with(std::slice::from_ref(&b.binder)), chunk);
                    Branch { label: Rc::from(b.label.as_str()), arity: 1, body }
                })
                .collect();
            Node::CoMatch(Rc::new(branches))
        }
        Term::Co(e) => Node::Co(compile_coterm(e, scope, chunk)),
    };
    chunk.push(node)
}

fn compile_coterm(e: &CoTerm, scope: &Scope, chunk: &mut Chunk) -> NodeId {
    let node = match e {
        CoTerm::Covar(a) => match scope.index(a) {
            Some(i) => Node::CoLocal(i),
            None => Node::CoDynamic(Rc::from(a.as_str())),
        },
        CoTerm::App(v, tail) => {
            let v = compile_ir(v, scope, chunk);
            let tail = compile_coterm(tail, scope, chunk);
            Node::App(v, tail)
        }
        CoTerm::MuTilde(x, c) => {
            let body = compile_cmd(c, &scope.with(std::slice::from_ref(x)), chunk);
            Node::MuTilde(body)
        }
        CoTerm::Prj(index) => Node::Prj(*index),
        CoTerm::CoCase(branches) => {
            let branches = branches.iter().map(|b| compile_branch(b, scope, chunk)).collect();
            Node::CoCase(Rc::new(branches))
        }
        CoTerm::MuTildeTensor(binders, c) => {
            let body = compile_cmd(c, &scope.with(binders), chunk);
            Node::MuTildeTensor(binders.len(), body)
        }
        CoTerm::Dtor(label, e) => {
            Node::Dtor(Rc::from(label.as_str()), compile_coterm(e, scope, chunk))
        }
    };
    chunk.push(node)
}

fn compile_branch(b: &CoCaseBranch, scope: &Scope, chunk: &mut Chunk) -> Branch {
    let body = compile_cmd(&b.body, &scope.with(&b.binders), chunk);
    Branch { label: Rc::from(b.label.as_str()), arity: b.binders.len(), body }
}

fn compile_cmd(c: &Command, scope: &Scope, chunk: &mut Chunk) -> NodeId {
    let node = match c {
        Command::Cut(t, e) => {
            let t = compile_ir(t, scope, chunk);
            let e = compile_coterm(e, scope, chunk);
            Node::Cut(t, e)
        }
    };
    chunk.push(node)
}
