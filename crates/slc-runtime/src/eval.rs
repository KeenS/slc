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
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Unbound(x) => write!(f, "unbound variable: {x}"),
            EvalError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
            EvalError::Diverged => write!(f, "evaluation diverged (fuel exhausted)"),
            EvalError::NoReduction => write!(f, "no applicable reduction"),
            EvalError::Exit(code) => write!(f, "exit({code})"),
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
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        // Only the `$`-prefixed encodings are reserved: they cannot be
        // written in the surface, so they never shadow a binding.
        "$unit" => return Ok(Value::Unit),
        "$no_args" => return Ok(Value::NoArguments),
        _ => {}
    }
    env.lookup(x).ok_or_else(|| EvalError::Unbound(x.to_string()))
}

/// Bind the components of a right-nested product onto the positional chain,
/// `arity` of them. The first component is pushed first (deepest) and the
/// last takes whatever remains and sits innermost, matching the order the
/// compiler assigned the consumer's binders.
pub(crate) fn bind_components(arity: usize, value: Value, env: &mut Env) -> Result<(), EvalError> {
    let mut rest = value;
    for index in 0..arity {
        if index + 1 == arity {
            env.define_local(rest);
            return Ok(());
        }
        let Value::Pair(head, tail) = rest else {
            return Err(EvalError::TypeMismatch(format!(
                "a consumer of {arity} components received {}",
                rest.display()
            )));
        };
        env.define_local(*head);
        rest = *tail;
    }
    Ok(())
}

/// Can this value receive an argument — as a function or as a consumer?
pub(crate) fn is_applicable(v: &Value) -> bool {
    matches!(
        v,
        Value::Closure { .. }
            | Value::CoCase { .. }
            | Value::CoTensor { .. }
            | Value::CoAbs { .. }
            | Value::Kont(_)
            | Value::Operation { .. }
            | Value::Resume(_)
            | Value::Builtin(_)
    )
}

pub(crate) fn collect_args(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Pair(a, b) => {
            collect_args(a, out);
            collect_args(b, out);
        }
        // A call with no arguments contributes none; unit is a value like
        // any other and contributes one.
        Value::NoArguments => {}
        other => out.push(other.clone()),
    }
}

/// The number of arguments each builtin expects.
pub(crate) fn builtin_arity(name: &str) -> usize {
    match name {
        "println" | "print" | "str_len" | "int_to_str" | "is_digit" | "is_ws" | "neg"
        | "file_exists" => 1,
        "__index" => 2,
        "map_len" => 1,
        "map_insert" => 3,
        "set_contains" => 2,
        "close_file" => 1,
        "set_insert" => 2,
        "set_len" => 1,
        "path_join" => 2,
        "list_push" => 2,
        "add" | "sub" | "mul" | "div" | "rem" | "eq" | "ne" | "lt" | "gt" | "le" | "ge"
        | "str_concat" | "str_eq" | "skip_digits" | "skip_ws" => 2,
        "substring" => 3,
        // Builtins that offer their outcome to continuations: the value
        // arguments come first, then one continuation per outcome.
        "read_file" | "open_file" | "read_line" => 3,
        "char_at" | "list_get" | "map_get" | "write_file" | "parse_int" => 4,
        "find_char" => 5,
        "__if_dispatch" => 3,
        "__handle" => 2,
        "list_new" => 0,
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
        "read_file" => {
            let (ok, failed) = (value(1), value(2));
            wrap(match crate::builtins::apply_io_builtin("read_file", &args[..1.min(args.len())]) {
                Ok(contents) => activate(ok, contents),
                Err(e) => activate(failed, message(e.to_string())),
            })
        }
        "open_file" => {
            let (opened, failed) = (value(1), value(2));
            let Value::Str(path) = value(0) else {
                return Err(EvalError::TypeMismatch("open_file expects a String".into()));
            };
            wrap(match crate::builtins::open_file(&path) {
                Ok(handle) => activate(opened, handle),
                Err(e) => activate(failed, message(e.to_string())),
            })
        }
        "read_line" => {
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
        "write_file" => {
            let (ok, failed) = (value(2), value(3));
            wrap(
                match crate::builtins::apply_io_builtin("write_file", &args[..2.min(args.len())]) {
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
        "list_get" => {
            let (ok, out_of_range) = (value(2), value(3));
            let (Value::List(items), Value::Int(index)) = (value(0), value(1)) else {
                return Err(EvalError::TypeMismatch("list_get expects (list, i64)".into()));
            };
            wrap(match usize::try_from(index).ok().and_then(|i| items.get(i).cloned()) {
                Some(item) => activate(ok, item),
                None => activate(
                    out_of_range,
                    message(format!(
                        "index {index} is out of range for a list of length {}",
                        items.len()
                    )),
                ),
            })
        }
        "map_get" => {
            let (found, missing) = (value(2), value(3));
            let (Value::Map(entries), key) = (value(0), value(1)) else {
                return Err(EvalError::TypeMismatch("map_get expects (map, key)".into()));
            };
            wrap(match entries.iter().find(|(k, _)| k == &key) {
                Some((_, v)) => activate(found, v.clone()),
                None => activate(missing, message(format!("no entry for {}", key.display()))),
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
    if matches!(name, "file_exists" | "close_file") {
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
    fn eval_bool() {
        let t = Term::Var("true".into());
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval(&t, &mut env, &mut fuel).unwrap(), Value::Bool(true));
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
        let t =
            Term::Pair(Box::new(Term::Var("$int_1".into())), Box::new(Term::Var("$int_2".into())));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(
            eval(&t, &mut env, &mut fuel).unwrap(),
            Value::Pair(Box::new(Value::Int(1)), Box::new(Value::Int(2)))
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
    fn eval_colam_binds() {
        // ⟨ int_5 ∥ λ̄x. ⟨ x ∥ k ⟩ ⟩ → 5
        let inner = Command::Cut(Term::Var("x".into()), CoTerm::Covar("k".into()));
        let c =
            Command::Cut(Term::Var("$int_5".into()), CoTerm::CoLam("x".into(), Box::new(inner)));
        let mut env = Env::new();
        let mut fuel = 100;
        assert_eq!(eval_command(&c, &mut env, &mut fuel).unwrap(), Value::Int(5));
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
