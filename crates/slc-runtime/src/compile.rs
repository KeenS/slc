//! Compile core λ̄μμ̃ to the closed IR: resolve each lexical (co-)variable to
//! a de Bruijn index, once.
//!
//! The compiler carries a scope — the binders in force, innermost last, term
//! and co-variables together, since they share one run-time environment. A
//! reference resolves to `Local`/`CoLocal` when its name is in scope, and to
//! `Dynamic`/`CoDynamic` otherwise (a literal, a global, or a pattern
//! variable the match engine injects at run time). A binder contributes its
//! names to the scope of its body only.

use crate::ir::{IBranch, ICoTerm, ICommand, Ir};
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

/// Compile a core term with no binders in scope.
pub(crate) fn compile_term(t: &Term) -> Rc<Ir> {
    compile_ir(t, &Scope::default())
}

/// Compile a core command with no binders in scope.
pub(crate) fn compile_command(c: &Command) -> Rc<ICommand> {
    compile_cmd(c, &Scope::default())
}

fn compile_ir(t: &Term, scope: &Scope) -> Rc<Ir> {
    Rc::new(match t {
        Term::Var(x) => match scope.index(x) {
            Some(i) => Ir::Local(i),
            None => Ir::Dynamic(Rc::from(x.as_str())),
        },
        Term::Lam(x, body) => Ir::Lam(compile_ir_under(body, scope, std::slice::from_ref(x))),
        Term::Mu(a, c) => Ir::Mu(compile_cmd(c, &scope.with(std::slice::from_ref(a)))),
        Term::Pair(a, b) => Ir::Pair(compile_ir(a, scope), compile_ir(b, scope)),
        Term::Inl(t) => Ir::Inl(compile_ir(t, scope)),
        Term::Inr(t) => Ir::Inr(compile_ir(t, scope)),
        Term::Tag(label, payload) => Ir::Tag(Rc::from(label.as_str()), compile_ir(payload, scope)),
        Term::CoAbs(a, body) => Ir::CoAbs(compile_ir_under(body, scope, std::slice::from_ref(a))),
        Term::Co(e) => Ir::Co(compile_coterm(e, scope)),
    })
}

/// Compile a term whose enclosing binder adds `extra` names to its scope.
fn compile_ir_under(t: &Term, scope: &Scope, extra: &[String]) -> Rc<Ir> {
    compile_ir(t, &scope.with(extra))
}

fn compile_coterm(e: &CoTerm, scope: &Scope) -> Rc<ICoTerm> {
    Rc::new(match e {
        CoTerm::Covar(a) => match scope.index(a) {
            Some(i) => ICoTerm::CoLocal(i),
            None => ICoTerm::CoDynamic(Rc::from(a.as_str())),
        },
        CoTerm::CoLam(x, c) => ICoTerm::CoLam(compile_cmd(c, &scope.with(std::slice::from_ref(x)))),
        CoTerm::MuTilde(x, c) => {
            ICoTerm::MuTilde(compile_cmd(c, &scope.with(std::slice::from_ref(x))))
        }
        CoTerm::Par(a, b) => ICoTerm::Par(compile_coterm(a, scope), compile_coterm(b, scope)),
        CoTerm::Fst => ICoTerm::Fst,
        CoTerm::Snd => ICoTerm::Snd,
        CoTerm::CoCase(branches) => {
            ICoTerm::CoCase(branches.iter().map(|b| compile_branch(b, scope)).collect())
        }
        CoTerm::MuTildeTensor(binders, c) => {
            ICoTerm::MuTildeTensor(binders.len(), compile_cmd(c, &scope.with(binders)))
        }
    })
}

fn compile_branch(b: &CoCaseBranch, scope: &Scope) -> IBranch {
    IBranch {
        label: Rc::from(b.label.as_str()),
        arity: b.binders.len(),
        body: compile_cmd(&b.body, &scope.with(&b.binders)),
    }
}

fn compile_cmd(c: &Command, scope: &Scope) -> Rc<ICommand> {
    Rc::new(match c {
        Command::Cut(t, e) => ICommand::Cut(compile_ir(t, scope), compile_coterm(e, scope)),
        // `Command(x, t)` binds nothing at run time; `x` is vestigial.
        Command::Command(_, t) => ICommand::Command(compile_ir(t, scope)),
        Command::Activate(k, v) => ICommand::Activate(compile_ir(k, scope), compile_ir(v, scope)),
    })
}
