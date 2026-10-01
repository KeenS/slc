//! Compile-time evaluation of a closed core term on the chunk machine.

use std::panic::{AssertUnwindSafe, catch_unwind};

use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::substitution::free_vars_term;
use slc_core::term::Term;

use crate::eval::eval;
use crate::value::{Env, Value, install_stdlib};

/// A folded constant, or a term the folder will not reduce.
#[derive(Debug, Clone, PartialEq)]
pub enum Fold {
    Value(Folded),
    Residual,
}

/// Data the folder may re-embed as a core literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Folded {
    Int(i64),
    Float(f64),
    Char(char),
    Bool(bool),
    Str(String),
    Unit,
    Tuple(Vec<Folded>),
    Tagged { label: String, payload: Box<Folded> },
}

/// Interpreter steps for one fold. Not the program's `--fuel`.
const FOLD_STEP_BUDGET: usize = 100_000;

/// Evaluate `term`. `fuel == 0` does not compile or run.
///
/// A positive `fuel` still uses [`FOLD_STEP_BUDGET`] steps. Effectful names,
/// names that are still open after `install_stdlib`, non-data values, errors,
/// and panics are [`Fold::Residual`].
pub fn try_fold(term: &Term, fuel: usize) -> Fold {
    if fuel == 0 {
        return Fold::Residual;
    }
    // A use under a binder is still a use. Free-variable search would miss it.
    if mentions_refused(term) {
        return Fold::Residual;
    }
    let mut env = Env::new();
    install_stdlib(&mut env);
    if free_vars_term(term).iter().any(|name| {
        let literal = matches!(name.as_str(), "$unit" | "$force" | "$adapt")
            || name.starts_with("$int_")
            || name.starts_with("$float_")
            || name.starts_with("$str_")
            || name.starts_with("$char_");
        !literal && env.lookup(name).is_none()
    }) {
        return Fold::Residual;
    }
    let ran = catch_unwind(AssertUnwindSafe(|| {
        let mut steps = FOLD_STEP_BUDGET;
        eval(term, &mut env, &mut steps)
    }));
    match ran {
        Ok(Ok(value)) => fold_value(&value).map_or(Fold::Residual, Fold::Value),
        _ => Fold::Residual,
    }
}

/// The core literal `value` re-embeds as, matching lowering and `literal_or_lookup`.
pub fn embed(value: &Folded) -> Term {
    match value {
        Folded::Int(n) => Term::Var(format!("$int_{n}")),
        Folded::Float(n) => Term::Var(format!("$float_{n}")),
        Folded::Char(c) => Term::Var(format!("$char_{c}")),
        Folded::Str(text) => Term::Var(format!("$str_{text:?}")),
        Folded::Unit => Term::Var("$unit".into()),
        Folded::Bool(value) => {
            let label = if *value { "Bool::True" } else { "Bool::False" };
            Term::Tag(label.into(), Box::new(Term::Var("$unit".into())))
        }
        Folded::Tuple(items) => Term::Tuple(items.iter().map(embed).collect()),
        Folded::Tagged { label, payload } => Term::Tag(label.clone(), Box::new(embed(payload))),
    }
}

fn refused_name(name: &str) -> bool {
    matches!(
        name,
        "EXIT"
            | "__handle"
            | "__read_file"
            | "__open_file"
            | "__read_line"
            | "__close_file"
            | "__write_file"
            | "__file_exists"
            | "__io_write"
            | "__io_write_line"
    )
}

fn mentions_refused(term: &Term) -> bool {
    fn term_has(term: &Term) -> bool {
        match term {
            Term::Var(name) => refused_name(name),
            Term::Lam(_, body) | Term::Tag(_, body) => term_has(body),
            Term::Mu(_, command) => command_has(command),
            Term::Tuple(items) => items.iter().any(term_has),
            Term::CoMatch { branches, .. } => {
                branches.iter().any(|branch| command_has(&branch.body))
            }
            Term::Co(co) => coterm_has(co),
        }
    }
    fn coterm_has(co: &CoTerm) -> bool {
        match co {
            CoTerm::Covar(name) => refused_name(name),
            CoTerm::App(arg, tail) => term_has(arg) || coterm_has(tail),
            CoTerm::MuTilde(_, command) => command_has(command),
            CoTerm::Prj(_) => false,
            CoTerm::CoCase { branches, .. } => {
                branches.iter().any(|branch| command_has(&branch.body))
            }
            CoTerm::MuTildeTensor(_, command) => command_has(command),
            CoTerm::Dtor(_, tail) => coterm_has(tail),
        }
    }
    fn command_has(command: &Command) -> bool {
        let Command::Cut(term, co) = command;
        term_has(term) || coterm_has(co)
    }
    term_has(term)
}

fn fold_value(value: &Value) -> Option<Folded> {
    match value {
        Value::Int(n) => Some(Folded::Int(*n)),
        Value::Float(n) => Some(Folded::Float(*n)),
        Value::Char(c) => Some(Folded::Char(*c)),
        Value::Str(text) => Some(Folded::Str(text.clone())),
        Value::Unit => Some(Folded::Unit),
        Value::Tuple(items) => {
            Some(Folded::Tuple(items.iter().map(fold_value).collect::<Option<_>>()?))
        }
        Value::Tagged(label, _) if label == "Bool::True" => Some(Folded::Bool(true)),
        Value::Tagged(label, _) if label == "Bool::False" => Some(Folded::Bool(false)),
        Value::Tagged(label, payload) => {
            Some(Folded::Tagged { label: label.clone(), payload: Box::new(fold_value(payload)?) })
        }
        Value::Closure { .. }
        | Value::Delayed { .. }
        | Value::Adapted { .. }
        | Value::Kont(_)
        | Value::Builtin(_)
        | Value::File(_)
        | Value::Operation { .. }
        | Value::Resume(_)
        | Value::PartialBuiltin(_, _)
        | Value::CoCase { .. }
        | Value::Menu { .. }
        | Value::CoTensor { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_core::command::Command;
    use slc_core::coterm::CoTerm;

    /// `μk. ⟨ name ∥ arg · k ⟩`. The binder receives the value the cut produces.
    fn applied(name: &str, arg: Term) -> Term {
        Term::Mu(
            "k".into(),
            Box::new(Command::Cut(
                Term::Var(name.into()),
                CoTerm::App(arg, Box::new(CoTerm::Covar("k".into()))),
            )),
        )
    }

    fn add_term() -> Term {
        applied("__add", Term::Tuple(vec![Term::Var("$int_1".into()), Term::Var("$int_2".into())]))
    }

    #[test]
    fn add_folds_to_three() {
        let Fold::Value(value) = try_fold(&add_term(), 1) else {
            panic!("expected a folded integer");
        };
        assert_eq!(value, Folded::Int(3));
        assert_eq!(embed(&value), Term::Var("$int_3".into()));
    }

    #[test]
    fn fuel_zero_is_residual() {
        assert!(matches!(try_fold(&add_term(), 0), Fold::Residual));
    }

    #[test]
    fn a_write_file_term_is_residual_and_creates_nothing() {
        let path = std::env::temp_dir().join("slc_try_fold_no_write");
        let _ = std::fs::remove_file(&path);
        let path_str = path.to_str().expect("utf-8 temp path");
        // Arity 4: path, bytes, then two continuations. A run would write before resuming.
        let term = applied(
            "__write_file",
            Term::Tuple(vec![
                Term::Var(format!("$str_{path_str:?}")),
                Term::Var("$str_\"data\"".into()),
                Term::Var("k".into()),
                Term::Var("k".into()),
            ]),
        );
        assert!(matches!(try_fold(&term, 1), Fold::Residual));
        assert!(!path.exists(), "folding must not create {}", path.display());
    }

    #[test]
    fn a_mu_that_returns_its_continuation_is_residual() {
        // `μk. ⟨ co(k) ∥ k ⟩` answers with the captured `Kont` and does not invoke it.
        let term = Term::Mu(
            "k".into(),
            Box::new(Command::Cut(
                Term::Co(Box::new(CoTerm::Covar("k".into()))),
                CoTerm::Covar("k".into()),
            )),
        );
        let mut env = Env::new();
        install_stdlib(&mut env);
        let mut steps = 1_000;
        let value = eval(&term, &mut env, &mut steps).expect("the capture runs");
        assert!(matches!(value, Value::Kont(_)), "machine produced {value:?}");
        assert!(matches!(try_fold(&term, 1), Fold::Residual));
    }

    #[test]
    fn negating_i64_min_is_residual() {
        let term = applied("__neg", Term::Var(format!("$int_{}", i64::MIN)));
        assert!(matches!(try_fold(&term, 1), Fold::Residual));
    }
}
