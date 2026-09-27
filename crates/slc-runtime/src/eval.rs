//! The evaluator's public face, and the runtime pieces the machine drives.
//!
//! Evaluation itself lives in `machine`: an abstract machine whose
//! continuation is data, which is what lets a captured continuation be
//! reinstated after its `mu` has answered, and more than once. This module
//! keeps the entry points, the error type, literal decoding, and the
//! builtin dispatch the machine calls into.

use crate::value::{Env, Value};
use slc_core::command::Command;
use slc_core::term::Term;

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    Unbound(String),
    TypeMismatch(String),
    Diverged,
    NoReduction,
    /// The top-level exit continuation was activated.
    Exit(i32),
    /// A `mu` continuation was jumped to under a handler whose prompt its
    /// captured stack does not hold.
    ForeignPrompt,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Unbound(x) => write!(f, "unbound variable: {x}"),
            EvalError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
            EvalError::Diverged => write!(f, "evaluation diverged (fuel exhausted)"),
            EvalError::NoReduction => write!(f, "no applicable reduction"),
            EvalError::Exit(code) => write!(f, "exit({code})"),
            EvalError::ForeignPrompt => write!(
                f,
                "a continuation left the handler it was captured under: it was jumped to under another"
            ),
        }
    }
}

impl std::error::Error for EvalError {}

/// Evaluate a term to a value. The core term is compiled to a flat chunk —
/// its lexical binders resolved to de Bruijn indices — which is installed for
/// the run and then executed from its root node.
pub fn eval(t: &Term, env: &mut Env, fuel: &mut usize) -> Result<Value, EvalError> {
    let (chunk, root) = crate::compile::compile_term(t);
    crate::chunk::with_chunk(chunk, || crate::machine::run_term(root, env, fuel))
}

/// Evaluate a command.
pub fn eval_command(c: &Command, env: &mut Env, fuel: &mut usize) -> Result<Value, EvalError> {
    let (chunk, root) = crate::compile::compile_command(c);
    crate::chunk::with_chunk(chunk, || crate::machine::run_command(root, env, fuel))
}

/// Apply a value to one argument. Ordinary application and continuation
/// activation are the same operation: a cut against something that consumes.
/// A chunk must already be installed (`chunk::with_chunk`).
pub fn apply_value(value: Value, arg: Value, fuel: &mut usize) -> Result<Value, EvalError> {
    crate::machine::run_apply(value, arg, fuel)
}

/// Apply, with the runtime's handler for `IO` installed beneath: the entry
/// point a program runs under, so an operation it never handles itself
/// reaches the outside world here.
pub fn apply_under_io(value: Value, arg: Value, fuel: &mut usize) -> Result<Value, EvalError> {
    crate::machine::run_apply_under_io(value, arg, fuel)
}

/// Run the node at `root` of the already-installed chunk. The driver compiles
/// the whole program into one chunk (`compile::compile_program`), installs it,
/// and runs each definition and `main` through here so they share it.
pub fn run_node(
    root: crate::chunk::NodeId,
    env: &mut Env,
    fuel: &mut usize,
) -> Result<Value, EvalError> {
    crate::machine::run_term(root, env, fuel)
}

/// A variable is a lowered literal, a builtin constant, or a binding.
pub(crate) fn literal_or_lookup(x: &str, env: &Env) -> Result<Value, EvalError> {
    match x {
        "$force" | "$adapt" => return Ok(Value::Builtin(x.into())),
        n if n.starts_with("$int_") => {
            let v: i64 = n[5..].parse().map_err(|_| EvalError::Unbound(x.to_string()))?;
            return Ok(Value::Int(v));
        }
        n if n.starts_with("$str_") => {
            let s = &n[5..];
            let s = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(s);
            let s = s
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
            return Ok(Value::Str(s));
        }
        n if n.starts_with("$float_") => {
            let v: f64 = n[7..].parse().map_err(|_| EvalError::Unbound(x.to_string()))?;
            return Ok(Value::Float(v));
        }
        n if n.starts_with("$char_") => {
            let c = n[6..].chars().next().ok_or_else(|| EvalError::Unbound(x.to_string()))?;
            return Ok(Value::Char(c));
        }
        // Only the `$`-prefixed encodings are reserved: they cannot be
        // written in the surface, so they never shadow a binding.
        "$unit" => return Ok(Value::Unit),
        _ => {}
    }
    env.lookup(x).ok_or_else(|| EvalError::Unbound(x.to_string()))
}

/// Bind a product's components onto the positional chain, `arity` of them:
/// one binder takes the whole value, and several take a tuple's components in
/// order — the order the compiler assigned the consumer's binders.
pub(crate) fn bind_components(arity: usize, value: Value, env: &mut Env) -> Result<(), EvalError> {
    match (arity, value) {
        (0, _) => {}
        (1, value) => env.define_local(value),
        (arity, Value::Tuple(items)) if items.len() == arity => {
            for item in items {
                env.define_local(item);
            }
        }
        (arity, value) => {
            return Err(EvalError::TypeMismatch(format!(
                "a consumer of {arity} components received {}",
                value.display()
            )));
        }
    }
    Ok(())
}

/// Can this value receive an argument — as a function or as a consumer?
pub(crate) fn is_applicable(v: &Value) -> bool {
    matches!(
        v,
        Value::Closure { .. }
            | Value::Delayed { .. }
            | Value::Adapted { .. }
            | Value::CoCase { .. }
            | Value::CoTensor { .. }
            | Value::Kont(_)
            | Value::Operation { .. }
            | Value::Resume(_)
            | Value::Builtin(_)
    )
}

/// The arguments a builtin receives from one application. A tuple is spread
/// into its components only for a builtin that takes several; to one that
/// takes one, the tuple is the argument.
pub(crate) fn collect_args(name: &str, v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Tuple(items) if builtin_arity(name) > 1 => out.extend(items.iter().cloned()),
        other => out.push(other.clone()),
    }
}

/// The number of arguments each builtin expects.
pub(crate) fn builtin_arity(name: &str) -> usize {
    match name {
        "__display" | "str_len" | "int_to_str" | "is_digit" | "is_ws" | "__neg"
        | "char_to_code" | "__file_exists" | "__to_i8" | "__to_i32" | "__to_i64" | "__to_u8"
        | "__to_u32" | "__to_u64" => 1,
        "__index" => 2,
        "__close_file" => 1,
        "__add" | "__sub" | "__mul" | "__div" | "__rem" | "__eq" | "__ne" | "__lt" | "__gt"
        | "__le" | "__ge" | "__wrapping_mul" | "__xor" | "str_concat" | "str_eq"
        | "skip_digits" | "skip_ws" => 2,
        "substring" => 3,
        // Builtins that offer their outcome to continuations: the value
        // arguments come first, then one continuation per outcome.
        "__read_file" | "__open_file" | "__read_line" => 3,
        // The runtime's own clauses for `IO`: the payload, then `resume`.
        "__io_write" | "__io_write_line" => 2,
        "__io_done" => 1,
        "char_at" | "__write_file" | "parse_int" => 4,
        "find_char" => 5,
        "__handle" => 2,
        "__enter_poly" => 2,
        "format" => 0, // variadic: apply immediately
        _ => 0,
    }
}

// note: __match_dispatch is variadic (scrutinee + N arms), arity 0

/// An offering builtin's outcome: the continuation to activate, with what.
fn activate(consumer: Value, outcome: Value) -> Result<(Value, Value), EvalError> {
    Ok((consumer, outcome))
}

fn wrap(step: Result<(Value, Value), EvalError>) -> Result<Option<(Value, Value)>, EvalError> {
    step.map(Some)
}

pub(crate) fn run_offering_builtin(
    name: &str,
    args: &[Value],
) -> Result<Option<(Value, Value)>, EvalError> {
    let value = |index: usize| args.get(index).cloned().unwrap_or(Value::Unit);
    let message = |text: String| Value::Str(text);
    match name {
        // The runtime is the outermost handler for `IO`: these are its
        // clauses. Each writes, then resumes — the operation answers unit,
        // and the program carries on where it performed.
        "__io_write" | "__io_write_line" => {
            let resume = value(1);
            let text = match value(0) {
                Value::Str(text) => text,
                other => other.display(),
            };
            let newline = if name == "__io_write_line" { "\n" } else { "" };
            {
                use std::io::Write;
                let stdout = std::io::stdout();
                let mut lock = stdout.lock();
                write!(lock, "{text}{newline}")
                    .map_err(|e| EvalError::TypeMismatch(e.to_string()))?;
            }
            wrap(activate(resume, Value::Unit))
        }
        "parse_int" => {
            let (ok, invalid, overflow) = (value(1), value(2), value(3));
            wrap(match value(0) {
                Value::Str(text) => match text.parse::<i64>() {
                    Ok(n) => activate(ok, Value::Int(n)),
                    Err(e) => {
                        let out_of_range = e.to_string().contains("too large")
                            || e.to_string().contains("too small");
                        let (continuation, reason) = if out_of_range {
                            (overflow, format!("integer out of range: {text:?}"))
                        } else {
                            (invalid, format!("not an integer: {text:?}"))
                        };
                        activate(continuation, message(reason))
                    }
                },
                other => Err(EvalError::TypeMismatch(format!(
                    "parse_int expects a String, got {}",
                    other.display()
                ))),
            })
        }
        "__read_file" => {
            let (ok, failed) = (value(1), value(2));
            wrap(
                match crate::builtins::apply_io_builtin("__read_file", &args[..1.min(args.len())]) {
                    Ok(contents) => activate(ok, contents),
                    Err(e) => activate(failed, message(e.to_string())),
                },
            )
        }
        "__open_file" => {
            let (opened, failed) = (value(1), value(2));
            let Value::Str(path) = value(0) else {
                return Err(EvalError::TypeMismatch("open_file expects a String".into()));
            };
            wrap(match crate::builtins::open_file(&path) {
                Ok(handle) => activate(opened, handle),
                Err(e) => activate(failed, message(e.to_string())),
            })
        }
        "__read_line" => {
            let (line, end) = (value(1), value(2));
            let Value::File(id) = value(0) else {
                return Err(EvalError::TypeMismatch("read_line expects a file handle".into()));
            };
            wrap(match crate::builtins::read_line(id) {
                Ok(Some(text)) => activate(line, Value::Str(text)),
                Ok(None) => activate(end, Value::Unit),
                Err(e) => Err(EvalError::TypeMismatch(e.to_string())),
            })
        }
        "__write_file" => {
            let (ok, failed) = (value(2), value(3));
            wrap(
                match crate::builtins::apply_io_builtin("__write_file", &args[..2.min(args.len())])
                {
                    Ok(_) => activate(ok, Value::Unit),
                    Err(e) => activate(failed, message(e.to_string())),
                },
            )
        }
        "char_at" => {
            let (ok, out_of_range) = (value(2), value(3));
            let (Value::Str(text), Value::Int(index)) = (value(0), value(1)) else {
                return Err(EvalError::TypeMismatch("char_at expects (String, i64)".into()));
            };
            wrap(match usize::try_from(index).ok().and_then(|i| text.chars().nth(i)) {
                Some(c) => activate(ok, Value::Char(c)),
                None => activate(
                    out_of_range,
                    message(format!(
                        "index {index} is out of range for a string of length {}",
                        text.chars().count()
                    )),
                ),
            })
        }
        "find_char" => {
            let (found, absent) = (value(3), value(4));
            let (Value::Str(text), Value::Int(from), Value::Int(target)) =
                (value(0), value(1), value(2))
            else {
                return Err(EvalError::TypeMismatch("find_char expects (String, i64, i64)".into()));
            };
            let Some(needle) = u32::try_from(target).ok().and_then(char::from_u32) else {
                return Err(EvalError::TypeMismatch(format!(
                    "find_char expects a character code, got {target}"
                )));
            };
            let start = usize::try_from(from).unwrap_or(0);
            wrap(match text.chars().enumerate().skip(start).find(|(_, c)| *c == needle) {
                Some((index, _)) => activate(found, Value::Int(index as i64)),
                None => activate(
                    absent,
                    message(format!("{needle:?} does not occur from index {from}")),
                ),
            })
        }
        _ => Ok(None),
    }
}

/// An ordinary builtin: its outcome is the returned value.
pub(crate) fn run_builtin_function(name: &str, args: Vec<Value>) -> Result<Value, EvalError> {
    if name == "EXIT" {
        return match args.into_iter().next() {
            Some(Value::Int(code)) => Err(EvalError::Exit(
                code.try_into()
                    .map_err(|_| EvalError::TypeMismatch("EXIT status must fit in i32".into()))?,
            )),
            other => Err(EvalError::TypeMismatch(format!(
                "EXIT expects an integer status, got {}",
                other.map(|v| v.display()).unwrap_or_else(|| "no argument".into())
            ))),
        };
    }
    // The `return` clause of the runtime's `IO` handler: a program's value
    // passes through it unchanged.
    if name == "__io_done" {
        return Ok(args.into_iter().next().unwrap_or(Value::Unit));
    }
    if matches!(name, "__file_exists" | "__close_file") {
        return crate::builtins::apply_io_builtin(name, &args)
            .map_err(|e| EvalError::TypeMismatch(e.to_string()));
    }
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    crate::builtins::apply_builtin(name, &args, &mut lock)
        .map_err(|e| EvalError::TypeMismatch(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_core::coterm::CoTerm;

    #[test]
    fn eval_int_literal() {
        let t = Term::Var("$int_42".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Int(42));
    }

    #[test]
    fn eval_string_literal() {
        let t = Term::Var("$str_hello".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Str("hello".into()));
    }

    #[test]
    fn eval_unit() {
        let t = Term::Var("$unit".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Unit);
    }

    #[test]
    fn eval_lambda() {
        let t = Term::Lam("x".into(), Box::new(Term::Var("x".into())));
        let mut env = Env::new();
        let mut fuel = 100;
        let v = eval(&t, &mut env, &mut fuel).unwrap();
        assert!(matches!(v, Value::Closure { .. }));
    }

    #[test]
    fn eval_pair() {
        let t = Term::Tuple(vec![Term::Var("$int_1".into()), Term::Var("$int_2".into())]);
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(
            eval(&t, &mut env, &mut fuel).unwrap(),
            Value::Tuple(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn eval_unbound() {
        let t = Term::Var("missing".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert!(matches!(eval(&t, &mut env, &mut fuel), Err(EvalError::Unbound(_))));
    }

    #[test]
    fn eval_fuel_exhaustion() {
        let t = Term::Var("$int_1".into());
        let mut env = Env::new();
        let mut fuel = 0;
        assert!(matches!(eval(&t, &mut env, &mut fuel), Err(EvalError::Diverged)));
    }

    #[test]
    fn eval_cut_returns_term() {
        let c = Command::Cut(Term::Var("$int_7".into()), CoTerm::Covar("k".into()));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval_command(&c, &mut env, &mut fuel).unwrap(), Value::Int(7));
    }

    #[test]
    fn eval_application_stack() {
        // ⟨ λx.x ∥ $int_5 · k ⟩ → 5
        let c = Command::Cut(
            Term::Lam("x".into(), Box::new(Term::Var("x".into()))),
            CoTerm::App(Term::Var("$int_5".into()), Box::new(CoTerm::Covar("k".into()))),
        );
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval_command(&c, &mut env, &mut fuel).unwrap(), Value::Int(5));
    }

    #[test]
    fn a_request_runs_one_menu_branch() {
        // ⟨ μ[C; .C::a(out). ⟨$int_1 ∥ out⟩ | .C::b(out). ⟨$int_2 ∥ out⟩]
        //   ∥ .C::b(μ̃x. ⟨x ∥ k⟩) ⟩ → 2
        let branch = |label: &str, lit: &str| slc_core::term::CoMatchBranch {
            label: label.into(),
            binder: "out".into(),
            body: Box::new(Command::Cut(Term::Var(lit.into()), CoTerm::Covar("out".into()))),
        };
        let menu = Term::CoMatch {
            owner: "C".into(),
            branches: vec![branch("C::a", "$int_1"), branch("C::b", "$int_2")],
        };
        let forward = CoTerm::MuTilde(
            "x".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        );
        let c = Command::Cut(menu, CoTerm::Dtor("C::b".into(), Box::new(forward)));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval_command(&c, &mut env, &mut fuel).unwrap(), Value::Int(2));
    }

    #[test]
    fn a_continuation_survives_its_mu() {
        // ⟨ μk. ⟨ co(μ̃x. ⟨x ∥ k⟩) ∥ k ⟩ ∥ μ̃c. … ⟩ — the μ answers with a
        // consumer that forwards to k; activating that consumer *after* the
        // μ has answered re-enters its continuation.
        let forward = CoTerm::MuTilde(
            "x".into(),
            Box::new(Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()))),
        );
        let mu = Term::Mu(
            "k".into(),
            Box::new(Command::Cut(Term::Co(Box::new(forward)), CoTerm::Covar("k".into()))),
        );
        // let c = μ…; if c is a consumer, send 9 into it — the send lands
        // back at the μ's continuation, so the whole term is 9.
        let body = Command::Cut(Term::Var("$int_9".into()), CoTerm::Covar("c".into()));
        let program = Command::Cut(mu, CoTerm::MuTilde("c".into(), Box::new(body)));
        let mut env = Env::new();
        let mut fuel = 1000;
        assert_eq!(eval_command(&program, &mut env, &mut fuel).unwrap(), Value::Int(9));
    }

    #[test]
    fn nested_mu_escapes_unwind_to_their_own_binders() {
        // ⟨ 1 ∥ μouter. ⟨ μinner. ⟨ 2 ∥ inner ⟩ ∥ outer ⟩ ⟩
        let inner_escape = Command::Cut(Term::Var("$int_2".into()), CoTerm::Covar("inner".into()));
        let inner = Term::Mu("inner".into(), Box::new(inner_escape));
        let outer_body = Command::Cut(inner, CoTerm::Covar("outer".into()));
        let term = Term::Mu("outer".into(), Box::new(outer_body));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&term, &mut env, &mut fuel).unwrap(), Value::Int(2));

        // If the outer binder is activated inside the inner μ, the inner
        // μ does not catch it: ⟨ 3 ∥ μouter. ⟨ μinner. ⟨ 4 ∥ outer ⟩ ∥ k ⟩ ⟩.
        let outer_escape = Command::Cut(Term::Var("$int_4".into()), CoTerm::Covar("outer".into()));
        let inner = Term::Mu("inner".into(), Box::new(outer_escape));
        let outer_body = Command::Cut(inner, CoTerm::Covar("k".into()));
        let term = Term::Mu("outer".into(), Box::new(outer_body));
        assert_eq!(eval(&term, &mut env, &mut fuel).unwrap(), Value::Int(4));
    }
}
