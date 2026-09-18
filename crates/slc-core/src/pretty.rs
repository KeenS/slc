//! Pretty-printing for core IR.

use crate::command::Command;
use crate::coterm::CoTerm;
use crate::term::Term;
use crate::types::{Base, Row, Type};

impl std::fmt::Display for Base {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Base::I32 => write!(f, "i32"),
            Base::I64 => write!(f, "i64"),
            Base::U32 => write!(f, "u32"),
            Base::U64 => write!(f, "u64"),
            Base::F64 => write!(f, "f64"),
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
            Type::Delayed(inner, row) => write!(f, "Delayed<{inner}, {row}>"),
            Type::Pos(b) => write!(f, "+{b}"),
            Type::Neg(b) => write!(f, "-{b}"),
            Type::Tensor(xs) => connective(f, xs, ", ", "(,)"),
            // A function is the two-component `;` whose first half is the
            // consumer of its argument, and prints as the function it is.
            Type::Par(xs) => match xs.as_slice() {
                [argument, result]
                    if argument.is_negative()
                        && !matches!(argument, Type::Var(_) | Type::Param(_))
                        && *result != Type::BOTTOM =>
                {
                    write!(f, "({} -> {result})", argument.dual())
                }
                _ => connective(f, xs, " ; ", "(;)"),
            },
            Type::Dual(t) => write!(f, "dual({t})"),
            Type::With(xs) => connective(f, xs, " & ", "(&)"),
            Type::Sum(xs) => connective(f, xs, " | ", "(|)"),
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
            // A row prints inside the parentheses of the type it is on, as
            // the surface writes it: `(i64 -> i64 / {Exn})`.
            Type::Rowed(t, row) => {
                let inner = t.to_string();
                match inner.strip_prefix('(').and_then(|body| body.strip_suffix(')')) {
                    Some(body) => write!(f, "({body} / {row})"),
                    None => write!(f, "({inner} / {row})"),
                }
            }
        }
    }
}

impl std::fmt::Display for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = self.effects.iter().map(ToString::to_string).collect();
        if let Some(tail) = self.tail {
            parts.push(format!("..?{tail}"));
        }
        write!(f, "{{{}}}", parts.join(", "))
    }
}

impl std::fmt::Display for crate::types::Effect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.name)?;
        if let Some((first, rest)) = self.args.split_first() {
            write!(formatter, "<{first}")?;
            for argument in rest {
                write!(formatter, ", {argument}")?;
            }
            write!(formatter, ">")?;
        }
        Ok(())
    }
}

/// A connective's components joined by its separator, in parentheses — or
/// its unit, the separator alone, when there are none: the surface's own
/// spelling, so a diagnostic shows a type the way a program writes it.
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
