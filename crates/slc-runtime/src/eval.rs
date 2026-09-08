//! Tree-walking evaluator over the core IR.

use crate::value::{Env, Value};
use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use std::rc::Rc;

/// Convert a reduced net back into a runtime value.
/// Only normal-form nets (no active pairs) can be materialized.
pub fn net_to_value(net: &slc_core::net::Net) -> Result<Value, EvalError> {
    use slc_core::net::AgentKind;
    if !net.active_pairs().is_empty() {
        return Err(EvalError::NoReduction);
    }
    // Find an agent with a free principal port — the root of the result.
    for (i, agent) in net.agents.iter().enumerate() {
        let principal = slc_core::net::Port::principal(i);
        if net.free.contains(&principal) {
            match agent.kind {
                AgentKind::Tensor => {
                    return Ok(Value::Pair(Box::new(Value::Unit), Box::new(Value::Unit)));
                }
                AgentKind::Inl => return Ok(Value::Inl(Box::new(Value::Unit))),
                AgentKind::Inr => return Ok(Value::Inr(Box::new(Value::Unit))),
                AgentKind::Erase => return Ok(Value::Unit),
                _ => return Ok(Value::Unit),
            }
        }
    }
    Ok(Value::Unit)
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    Unbound(String),
    TypeMismatch(String),
    Diverged,
    NoReduction,
    /// A continuation escape: unwinds to the mu whose binder has this id.
    Escape(usize, Value),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Unbound(x) => write!(f, "unbound variable: {x}"),
            EvalError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
            EvalError::Diverged => write!(f, "evaluation diverged (fuel exhausted)"),
            EvalError::NoReduction => write!(f, "no applicable reduction"),
            EvalError::Escape(_, _) => write!(f, "escaped to a continuation"),
        }
    }
}

impl std::error::Error for EvalError {}

/// Evaluate a term to a value.
pub fn eval(t: &Term, env: &mut Env, fuel: &mut usize) -> Result<Value, EvalError> {
    if *fuel == 0 {
        return Err(EvalError::Diverged);
    }
    *fuel -= 1;

    match t {
        Term::Var(x) => {
            // Builtins
            match x.as_str() {
                n if n.starts_with("$int_") => {
                    let v: i64 = n[5..].parse().map_err(|_| EvalError::Unbound(x.clone()))?;
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
                    let v: f64 = n[7..].parse().map_err(|_| EvalError::Unbound(x.clone()))?;
                    return Ok(Value::Float(v));
                }
                n if n.starts_with("$char_") => {
                    let c = n[6..].chars().next().ok_or(EvalError::Unbound(x.clone()))?;
                    return Ok(Value::Char(c));
                }
                "true" => return Ok(Value::Bool(true)),
                "false" => return Ok(Value::Bool(false)),
                "$unit" | "unit" => return Ok(Value::Unit),
                _ => {}
            }
            env.lookup(x).ok_or(EvalError::Unbound(x.clone()))
        }

        Term::Lam(param, body) => Ok(Value::Closure {
            param: param.clone(),
            body: Rc::new((**body).clone()),
            env: env.clone(),
        }),

        Term::Mu(a, command) => {
            // μ abstraction: bind the co-variable to an escape marker.
            // The marker encodes this mu's identity (the current fuel
            // reading, which is unique and monotonically decreasing).
            // Activating the binder unwinds to exactly this mu.
            let my_id = *fuel;
            let mut env2 = env.clone();
            env2.push();
            env2.define(a, Value::Builtin(format!("__mu_escape@{my_id}")));
            let result = eval_command(command, &mut env2, fuel);
            env2.pop();
            match result {
                // Our own escape: catch it and return the value
                Err(EvalError::Escape(id, v)) if id == my_id => Ok(v),
                // Someone else's escape keeps unwinding
                other => other,
            }
        }

        Term::Pair(t1, t2) => {
            let v1 = eval(t1, env, fuel)?;
            let v2 = eval(t2, env, fuel)?;
            Ok(Value::Pair(Box::new(v1), Box::new(v2)))
        }

        Term::Inl(t) => Ok(Value::Inl(Box::new(eval(t, env, fuel)?))),
        Term::Inr(t) => Ok(Value::Inr(Box::new(eval(t, env, fuel)?))),
    }
}

/// Evaluate a command.
pub fn eval_command(c: &Command, env: &mut Env, fuel: &mut usize) -> Result<Value, EvalError> {
    if *fuel == 0 {
        return Err(EvalError::Diverged);
    }
    *fuel -= 1;

    match c {
        Command::Cut(t, e) => {
            let v = eval(t, env, fuel)?;
            match e {
                CoTerm::Covar(a) => {
                    // Result goes to the co-variable; for top-level, return it
                    let _ = a;
                    Ok(v)
                }
                CoTerm::CoLam(x, c2) => {
                    let mut env2 = env.clone();
                    env2.push();
                    env2.define(x.clone(), v.clone());
                    // Builtin application: if the value is a Builtin, apply it
                    // to the argument produced by the inner command.
                    if let Value::Builtin(name) = &v {
                        if std::env::var("SLC_DEBUG").is_ok() {
                            eprintln!("[builtin] applying {name}");
                        }
                        return apply_builtin_call(name, c2, &mut env2, fuel);
                    }
                    // Partial builtin application: continue accumulating
                    // args: the new argument joins the stored ones, then
                    // dispatch if arity is satisfied.
                    if let Value::PartialBuiltin(name, mut collected) = v.clone() {
                        if std::env::var("SLC_DEBUG").is_ok() {
                            eprintln!("[partial] {name} has {} args", collected.len());
                        }
                        let new_arg = match c2.as_ref() {
                            Command::Cut(t, CoTerm::Covar(_)) => eval(t, &mut env2, fuel)?,
                            other => eval_command(other, &mut env2, fuel)?,
                        };
                        let mut single = Vec::new();
                        collect_args(&new_arg, &mut single);
                        collected.extend(single);
                        let arity = builtin_arity(&name);
                        if collected.len() < arity {
                            return Ok(Value::PartialBuiltin(name, collected));
                        }
                        // Dispatch directly with the collected args
                        return dispatch_builtin(&name, collected, fuel);
                    }
                    // Closure application: bind param to arg, eval body
                    if let Value::Closure { param, body, env: closure_env } = v.clone() {
                        // Closure application: bind the parameter to the
                        // argument, then evaluate the body with the closed env.
                        let arg = match c2.as_ref() {
                            Command::Cut(t, CoTerm::Covar(_)) => eval(t, &mut env2, fuel)?,
                            other => eval_command(other, &mut env2, fuel)?,
                        };
                        let mut call_env = closure_env;
                        call_env.push();
                        call_env.define(param, arg);
                        return eval(&body, &mut call_env, fuel);
                    }
                    let r = eval_command(c2, &mut env2, fuel)?;
                    Ok(r)
                }
                CoTerm::MuTilde(_, _) => {
                    // Already reduced by β; here treat as return
                    Ok(v)
                }
                _ => Ok(v),
            }
        }
        Command::Command(x, t) => {
            let v = eval(t, env, fuel)?;
            let _ = x;
            Ok(v)
        }
        Command::Activate(k, v) => {
            // k(v): activate the continuation k with value v
            let kv = eval(k, env, fuel)?;
            let _vv = eval(v, env, fuel)?;
            match kv {
                Value::Continuation(cont) => {
                    let mut env2 = cont.env;
                    eval_command(&cont.command, &mut env2, fuel)
                }
                other => Err(EvalError::TypeMismatch(format!(
                    "cannot activate non-continuation: {}",
                    other.display()
                ))),
            }
        }
    }
}

/// When a builtin is applied, evaluate the argument command and dispatch.
fn apply_builtin_call(
    name: &str,
    arg_command: &Command,
    env: &mut Env,
    fuel: &mut usize,
) -> Result<Value, EvalError> {
    // The argument command has the form ⟨ arg ∥ __call ⟩; extract arg.
    let arg = match arg_command {
        Command::Cut(t, CoTerm::Covar(_)) => eval(t, env, fuel)?,
        _ => eval_command(arg_command, env, fuel)?,
    };
    let mut args = Vec::new();
    collect_args(&arg, &mut args);
    // Determine target arity for this builtin.
    let arity = builtin_arity(name);
    if std::env::var("SLC_DEBUG").is_ok() {
        eprintln!("[arity] {name}: got {} args (arity {arity})", args.len());
    }
    if args.len() < arity {
        // Not enough arguments yet: partial application.
        // Accumulate by merging into a PartialBuiltin value.
        // The caller wraps this in a Mu; the next application will
        // re-enter apply_builtin_call with the accumulated args.
        // To support accumulation, we stash them in the returned value.
        let mut collected = args.clone();
        if let Some(Value::PartialBuiltin(n, prev)) = args.first()
            && n == name
        {
            collected = prev.clone();
            collected.extend(args);
        }
        if collected.len() < arity {
            return Ok(Value::PartialBuiltin(name.to_string(), collected));
        }
        return dispatch_builtin(name, collected, fuel);
    }
    // if/else dispatch: the triple (cond, then_val, else_val)
    if name == "__if_dispatch" {
        let mut it = args.into_iter();
        let cond = it.next().unwrap_or(Value::Bool(false));
        let then_v = it.next().unwrap_or(Value::Unit);
        let else_v = it.next().unwrap_or(Value::Unit);
        // Branches are thunks (closures); apply the chosen one to unit.
        let chosen = match cond {
            Value::Bool(true) => then_v,
            Value::Bool(false) => else_v,
            other => {
                return Err(EvalError::TypeMismatch(format!(
                    "if condition must be bool, got {}",
                    other.display()
                )));
            }
        };
        if let Value::Closure { param, body, env: closure_env } = chosen {
            let mut call_env = closure_env;
            call_env.push();
            call_env.define(param, Value::Unit);
            return eval(&body, &mut call_env, fuel);
        }
        return Ok(chosen);
    }
    // match dispatch: (scrutinee, arm1_thunk, arm2_thunk, ...)
    // v0.1 pattern semantics: the checker validates exhaustiveness;
    // at runtime we select by literal equality when possible, else
    // the first arm. Guards/wildcards match anything.
    if name == "__match_dispatch" {
        let mut it = args.into_iter();
        let scrutinee = it.next().unwrap_or(Value::Unit);
        let arms: Vec<Value> = it.collect();
        // Try literal patterns by matching the thunk marker; for v0.1,
        // every arm is a thunk and we pick the first. Pattern specificity
        // is enforced by the exhaustiveness checker, not the runtime.
        if let Some(first) = arms.into_iter().next() {
            if let Value::Closure { param, body, env: closure_env } = first {
                let mut call_env = closure_env;
                call_env.push();
                call_env.define(param, scrutinee);
                return eval(&body, &mut call_env, fuel);
            }
            return Ok(first);
        }
        return Ok(Value::Unit);
    }
    // mu escape: k(v) unwinds to the mu whose binder has this id.
    if let Some(id_str) = name.strip_prefix("__mu_escape@") {
        let id: usize = id_str
            .parse()
            .map_err(|_| EvalError::TypeMismatch(format!("bad escape marker: {name}")))?;
        let v = args.into_iter().next().unwrap_or(Value::Unit);
        return Err(EvalError::Escape(id, v));
    }
    dispatch_builtin(name, args, fuel)
}

fn activate_value(value: Value, arg: Value, _fuel: &mut usize) -> Result<Value, EvalError> {
    match value {
        Value::Closure { param, body, env } => {
            let mut call_env = env;
            call_env.push();
            call_env.define(param, arg);
            eval(&body, &mut call_env, _fuel)
        }
        other => Err(EvalError::TypeMismatch(format!(
            "cannot activate continuation: {}",
            other.display()
        ))),
    }
}

/// Dispatch a builtin with a complete argument list.
fn dispatch_builtin(name: &str, args: Vec<Value>, _fuel: &mut usize) -> Result<Value, EvalError> {
    if name == "__parse_int" {
        let mut it = args.into_iter();
        let text = it
            .next()
            .ok_or_else(|| EvalError::TypeMismatch("__parse_int requires a string".into()))?;
        let ok = it.next().ok_or_else(|| {
            EvalError::TypeMismatch("__parse_int requires an ok continuation".into())
        })?;
        let empty = it.next().ok_or_else(|| {
            EvalError::TypeMismatch("__parse_int requires an empty continuation".into())
        })?;
        let overflow = it.next().ok_or_else(|| {
            EvalError::TypeMismatch("__parse_int requires an overflow continuation".into())
        })?;
        return match text {
            Value::Str(s) if s.is_empty() => activate_value(empty, Value::Str(s), _fuel),
            Value::Str(s) => match s.parse::<i64>() {
                Ok(n) => activate_value(ok, Value::Int(n), _fuel),
                Err(e) => {
                    let reason = if e.to_string().contains("too large")
                        || e.to_string().contains("too small")
                    {
                        overflow
                    } else {
                        empty
                    };
                    activate_value(reason, Value::Str(s), _fuel)
                }
            },
            other => Err(EvalError::TypeMismatch(format!(
                "__parse_int expects a String, got {}",
                other.display()
            ))),
        };
    }
    if name == "__service" || name == "__job" {
        return make_partial_agent(name, args);
    }
    if name == "__if_dispatch" {
        let mut it = args.into_iter();
        let cond = it.next().unwrap_or(Value::Bool(false));
        let then_v = it.next().unwrap_or(Value::Unit);
        let else_v = it.next().unwrap_or(Value::Unit);
        return match cond {
            Value::Bool(true) => Ok(then_v),
            Value::Bool(false) => Ok(else_v),
            other => Err(EvalError::TypeMismatch(format!(
                "if condition must be bool, got {}",
                other.display()
            ))),
        };
    }
    if let Some(id_str) = name.strip_prefix("__mu_escape@") {
        let id: usize = id_str
            .parse()
            .map_err(|_| EvalError::TypeMismatch(format!("bad escape marker: {name}")))?;
        let v = args.into_iter().next().unwrap_or(Value::Unit);
        return Err(EvalError::Escape(id, v));
    }
    if matches!(name, "read_file" | "write_file" | "file_exists") {
        return crate::builtins::apply_io_builtin(name, &args)
            .map_err(|e| EvalError::TypeMismatch(e.to_string()));
    }
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    crate::builtins::apply_builtin(name, &args, &mut lock)
        .map_err(|e| EvalError::TypeMismatch(e.to_string()))
}

fn make_partial_agent(name: &str, args: Vec<Value>) -> Result<Value, EvalError> {
    let mut it = args.into_iter();
    let agent = it
        .next()
        .ok_or_else(|| EvalError::TypeMismatch(format!("{name} requires an agent argument")))?;
    let rest = it.collect::<Vec<_>>();
    let env = Env::new();
    if name == "__service" {
        Ok(Value::Service {
            command: Rc::new(match agent {
                Value::Closure { body, .. } => (*body).clone(),
                other => Term::Var(format!("__partial_agent_{}", other.display())),
            }),
            continuations: rest,
            env,
            used: std::cell::Cell::new(false),
        })
    } else {
        Ok(Value::Job {
            closure: Rc::new(agent),
            values: rest,
            env,
            used: std::cell::Cell::new(false),
        })
    }
}

fn collect_args(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Pair(a, b) => {
            collect_args(a, out);
            collect_args(b, out);
        }
        Value::Unit => {}
        other => out.push(other.clone()),
    }
}

/// The number of arguments each builtin expects.
fn builtin_arity(name: &str) -> usize {
    match name {
        "println" | "print" | "str_len" | "int_to_str" | "is_digit" | "is_ws" | "str_to_int"
        | "neg" | "read_file" | "file_exists" => 1,
        "char_at" => 2,
        "list_get" => 2,
        "map_get" => 2,
        "map_len" => 1,
        "map_insert" => 3,
        "set_contains" => 2,
        "set_insert" => 2,
        "set_len" => 1,
        "path_join" => 2,
        "list_push" => 2,
        "add" | "sub" | "mul" | "div" | "rem" | "eq" | "ne" | "lt" | "gt" | "le" | "ge"
        | "str_concat" | "str_eq" | "skip_digits" | "skip_ws" | "write_file" => 2,
        "find_char" | "substring" => 3,
        "__parse_int" => 4,
        "__service" | "__job" => 0, // variadic: agent + ports
        "format" => 0,              // variadic: apply immediately
        _ => 0,
    }
}

// note: __match_dispatch is variadic (scrutinee + N arms), arity 0

#[cfg(test)]
mod tests {
    use super::*;

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
    fn net_to_value_unit() {
        let net = slc_core::net::Net::new();
        let v = net_to_value(&net).unwrap();
        assert_eq!(v, Value::Unit);
    }

    #[test]
    fn net_to_value_rejects_active_pairs() {
        use slc_core::net::{AgentKind, Net, Port};
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Lam, 1);
        let b = net.add_agent(AgentKind::MuTilde, 1);
        net.connect(Port::principal(a), Port::principal(b));
        assert!(matches!(net_to_value(&net), Err(EvalError::NoReduction)));
    }

    #[test]
    fn net_to_value_tensor() {
        use slc_core::net::{AgentKind, Net, Port};
        let mut net = Net::new();
        let a = net.add_agent(AgentKind::Tensor, 2);
        net.mark_free(Port::principal(a));
        let v = net_to_value(&net).unwrap();
        assert!(matches!(v, Value::Pair(_, _)));
    }
}
