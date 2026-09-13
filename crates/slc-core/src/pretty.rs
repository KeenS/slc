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
            Type::Tensor(xs) => connective(f, xs, " ⊗ ", "1"),
            Type::Par(xs) => connective(f, xs, " ⅋ ", "⊥"),
            Type::Dual(t) => write!(f, "dual({t})"),
            Type::With(xs) => connective(f, xs, " & ", "⊤"),
            Type::Sum(xs) => connective(f, xs, " + ", "0"),
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

/// A connective's components joined by its symbol, in parentheses — or its
/// unit, when there are none.
fn connective(
    f: &mut std::fmt::Formatter<'_>,
    components: &[Type],
    symbol: &str,
    unit: &str,
) -> std::fmt::Result {
    if components.is_empty() {
        return write!(f, "{unit}");
    }
    write!(f, "(")?;
    for (i, component) in components.iter().enumerate() {
        if i > 0 {
            write!(f, "{symbol}")?;
        }
        write!(f, "{component}")?;
    }
    write!(f, ")")
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Term::Var(x) => write!(f, "{x}"),
            Term::Lam(x, t) => write!(f, "λ{x}. {t}"),
            Term::Mu(a, c) => write!(f, "μ{a}. {c}"),
            Term::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ⊗ ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
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
