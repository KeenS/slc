//! Pretty-printing for core IR.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::term::Term;
use crate::types::{Base, Type};

impl std::fmt::Display for Base {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Base::I32 => write!(f, "i32"),
            Base::I64 => write!(f, "i64"),
            Base::U32 => write!(f, "u32"),
            Base::U64 => write!(f, "u64"),
            Base::Bool => write!(f, "bool"),
            Base::Str => write!(f, "String"),
            Base::Char => write!(f, "char"),
            Base::Unit => write!(f, "unit"),
            Base::File => write!(f, "File"),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Var(v) => write!(f, "?{v}"),
            Type::Pos(b) => write!(f, "+{b}"),
            Type::Neg(b) => write!(f, "-{b}"),
            Type::Tensor(a, b) => write!(f, "({a} ⊗ {b})"),
            Type::Par(a, b) => write!(f, "({a} ⅋ {b})"),
            Type::One => write!(f, "1"),
            Type::Bottom => write!(f, "⊥"),
            Type::Zero => write!(f, "0"),
            Type::Top => write!(f, "⊤"),
            Type::Dual(t) => write!(f, "dual({t})"),
            Type::With(a, b) => write!(f, "({a} & {b})"),
            Type::Sum(a, b) => write!(f, "({a} + {b})"),
            Type::Param(i) => write!(f, "%{i}"),
            Type::Named(name, args) => {
                write!(f, "{name}")?;
                if let Some((first, rest)) = args.split_first() {
                    write!(f, "<{first}")?;
                    for arg in rest {
                        write!(f, ", {arg}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
        }
    }
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Term::Var(x) => write!(f, "{x}"),
            Term::Lam(x, t) => write!(f, "λ{x}. {t}"),
            Term::Mu(a, c) => write!(f, "μ{a}. {c}"),
            Term::Pair(t1, t2) => write!(f, "({t1} ⊗ {t2})"),
            Term::Tag(label, t) => write!(f, "{label}({t})"),
            Term::CoMatch { owner, branches } => {
                write!(f, "μ[{owner}")?;
                if branches.is_empty() {
                    return write!(f, "]");
                }
                write!(f, "; ")?;
                for (i, branch) in branches.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, ".{}({}). {}", branch.label, branch.binder, branch.body)?;
                }
                write!(f, "]")
            }
            Term::Co(e) => write!(f, "co({e})"),
        }
    }
}

impl std::fmt::Display for CoTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoTerm::Covar(a) => write!(f, "{a}"),
            CoTerm::App(v, e) => write!(f, "{v} · {e}"),
            CoTerm::MuTilde(x, c) => write!(f, "μ̃{x}. {c}"),
            CoTerm::Prj(index) => write!(f, "prj:{index}"),
            CoTerm::MuTildeTensor(binders, c) => write!(f, "μ̃({}). {c}", binders.join(", ")),
            CoTerm::CoCase { owner, branches } => {
                write!(f, "μ̃[{owner}")?;
                if branches.is_empty() {
                    return write!(f, "]");
                }
                write!(f, "; ")?;
                for (i, branch) in branches.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}({}). {}", branch.label, branch.binders.join(", "), branch.body)?;
                }
                write!(f, "]")
            }
            CoTerm::Dtor(label, e) => write!(f, ".{label}({e})"),
        }
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Cut(t, e) => write!(f, "⟨{t} ∥ {e}⟩"),
        }
    }
}
