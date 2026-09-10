//! Tree-walking evaluator over the core IR.

use crate::value::{Env, Value};
use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    static NEXT_MU_ID: Cell<u64> = const { Cell::new(1) };
}

fn fresh_mu_id() -> Result<u64, EvalError> {
    NEXT_MU_ID.with(|id| {
        let next = id.get().checked_add(1).ok_or(EvalError::Diverged)?;
        let current = id.get();
        id.set(next);
        Ok(current)
    })
}

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
    Escape(u64, Value),
    /// The top-level EXIT continuation was activated.
    Exit(i32),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Unbound(x) => write!(f, "unbound variable: {x}"),
            EvalError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
            EvalError::Diverged => write!(f, "evaluation diverged (fuel exhausted)"),
            EvalError::NoReduction => write!(f, "no applicable reduction"),
            // Reaching the top means no `mu` was left to catch it: the one
            // that captured this continuation has already answered.
            EvalError::Escape(_, _) => write!(
                f,
                "a continuation was activated after the `mu` that captured it had answered; \
                 continuations escape here, they do not resume"
            ),
            EvalError::Exit(code) => write!(f, "exit({code})"),
        }
    }
}

impl std::error::Error for EvalError {}

#[derive(Debug, Clone, PartialEq)]
enum RuntimePattern {
    Wildcard,
    Binding(String, Box<RuntimePattern>),
    Literal(Value),
    /// An enum variant pattern: a label and the payload patterns it binds.
    Tagged(String, Vec<RuntimePattern>),
    Range(Box<RuntimePattern>, Box<RuntimePattern>),
    Or(Vec<RuntimePattern>),
    Tuple(Vec<RuntimePattern>),
    List {
        items: Vec<RuntimePattern>,
        rest: Option<(Option<String>, Box<RuntimePattern>)>,
    },
}

/// Match dispatch payloads use a top-level pair spine:
/// `(scrutinee, arm1, arm2, ...)`. Each arm is itself a nested pair
/// `(descriptor, (guard, thunk))`. Flattening all pairs destroys both a
/// pair-valued scrutinee and the nested arm structure, so split only this
/// outer spine.
fn split_match_payload(v: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let mut current = v.clone();
    while let Value::Pair(head, rest) = current {
        out.push((*head).clone());
        current = (*rest).clone();
    }
    out.push(current);
    out
}

/// Unwrap the additive marker used by lowered match arms.
fn unwrap_match_arm(v: &Value) -> Value {
    match v {
        Value::Inl(inner) | Value::Inr(inner) => (**inner).clone(),
        other => other.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Descriptor {
    Pattern(RuntimePattern),
    Rest,
}

fn parse_runtime_pattern(s: &str) -> Descriptor {
    if s == ".." {
        return Descriptor::Rest;
    }
    let mut chars = s.chars().peekable();
    Descriptor::Pattern(parse_runtime_pattern_inner(&mut chars))
}

fn parse_runtime_pattern_inner(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> RuntimePattern {
    match chars.next() {
        Some('*') => RuntimePattern::Wildcard,
        Some('.') if chars.peek() == Some(&'.') => {
            chars.next();
            RuntimePattern::Wildcard
        }
        Some('#') => {
            let text = take_while(chars, |c| c.is_ascii_digit() || *c == '-' || *c == '.');
            let rest = parse_runtime_pattern_tail(chars);
            match (text.parse::<i64>().ok(), text.parse::<f64>().ok(), rest) {
                (Some(start), _, Some(RuntimePattern::Literal(Value::Int(end)))) => {
                    RuntimePattern::Range(
                        Box::new(RuntimePattern::Literal(Value::Int(start))),
                        Box::new(RuntimePattern::Literal(Value::Int(end))),
                    )
                }
                (Some(n), _, _) => RuntimePattern::Literal(Value::Int(n)),
                (None, Some(n), _) => RuntimePattern::Literal(Value::Float(n)),
                _ => RuntimePattern::Wildcard,
            }
        }
        Some('%') => {
            let text = take_while(chars, |c| c.is_ascii_digit() || *c == '-' || *c == '.');
            let rest = parse_runtime_pattern_tail(chars);
            match (text.parse::<f64>().ok(), rest) {
                (Some(start), Some(RuntimePattern::Literal(Value::Float(end)))) => {
                    RuntimePattern::Range(
                        Box::new(RuntimePattern::Literal(Value::Float(start))),
                        Box::new(RuntimePattern::Literal(Value::Float(end))),
                    )
                }
                (Some(n), _) => RuntimePattern::Literal(Value::Float(n)),
                _ => RuntimePattern::Wildcard,
            }
        }
        Some('"') => {
            let text = take_quoted(chars, '"');
            // A quoted name followed by `(` is a variant pattern; the
            // parenthesized patterns match the variant payload.
            if chars.peek() == Some(&'(') {
                chars.next();
                let mut fields = Vec::new();
                loop {
                    match chars.peek() {
                        Some(')') | None => {
                            chars.next();
                            break;
                        }
                        _ => {}
                    }
                    fields.push(parse_runtime_pattern_inner(chars));
                    if chars.next() != Some(',') {
                        break;
                    }
                }
                return RuntimePattern::Tagged(text, fields);
            }
            RuntimePattern::Literal(Value::Str(text))
        }
        Some('\'') => {
            let c = match chars.next() {
                Some('\\') => chars.next().unwrap_or('\\'),
                Some(c) => c,
                None => '\0',
            };
            let _ = chars.next();
            let rest = parse_runtime_pattern_tail(chars);
            match rest {
                Some(RuntimePattern::Literal(Value::Char(end_c))) => RuntimePattern::Range(
                    Box::new(RuntimePattern::Literal(Value::Char(c))),
                    Box::new(RuntimePattern::Literal(Value::Char(end_c))),
                ),
                _ => RuntimePattern::Literal(Value::Char(c)),
            }
        }
        Some('(') => {
            let mut items = Vec::new();
            loop {
                match chars.peek() {
                    Some(')') | None => {
                        chars.next();
                        break;
                    }
                    _ => {}
                }
                items.push(parse_runtime_pattern_inner(chars));
                match chars.next() {
                    Some('|') => {
                        loop {
                            match chars.peek() {
                                Some(')') | None => {
                                    chars.next();
                                    break;
                                }
                                _ => {}
                            }
                            items.push(parse_runtime_pattern_inner(chars));
                            if chars.next() != Some('|') {
                                break;
                            }
                        }
                        return RuntimePattern::Or(items);
                    }
                    Some(',') => {}
                    _ => break,
                }
            }
            RuntimePattern::Tuple(items)
        }
        Some('$') => RuntimePattern::Binding(
            take_while(chars, |c| c.is_alphanumeric() || *c == '_'),
            Box::new(RuntimePattern::Wildcard),
        ),
        Some('[') => {
            let mut items = Vec::new();
            let rest = None;
            loop {
                match chars.peek() {
                    Some(']') | None => {
                        chars.next();
                        break;
                    }
                    _ => {}
                }
                items.push(parse_runtime_pattern_inner(chars));
                match chars.next() {
                    Some(',') => {
                        if chars.peek() == Some(&']') {
                            chars.next();
                            break;
                        }
                    }
                    Some(']') => break,
                    _ => break,
                }
            }
            if items.last() == Some(&RuntimePattern::Wildcard) {
                // no-op; a trailing wildcard is just an ordinary item
            }
            RuntimePattern::List { items, rest }
        }
        Some(c) => {
            let mut name = String::new();
            name.push(c);
            name.push_str(&take_while(chars, |c| {
                c.is_alphanumeric() || *c == '_' || *c == ':' || *c == '@'
            }));
            if let Some(stripped) = name.strip_suffix('@') {
                name = stripped.to_string();
                if chars.peek() == Some(&'*') {
                    chars.next();
                    return RuntimePattern::Binding(name, Box::new(RuntimePattern::Wildcard));
                }
                return RuntimePattern::Binding(name, Box::new(parse_runtime_pattern_inner(chars)));
            }
            if name == "true" {
                return RuntimePattern::Literal(Value::Bool(true));
            }
            if name == "false" {
                return RuntimePattern::Literal(Value::Bool(false));
            }
            RuntimePattern::Binding(name, Box::new(RuntimePattern::Wildcard))
        }
        None => RuntimePattern::Wildcard,
    }
}

fn parse_runtime_pattern_tail(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Option<RuntimePattern> {
    if chars.peek() == Some(&'.') {
        chars.next();
        if chars.next() != Some('.') {
            return None;
        }
        if chars.next() != Some('=') {
            return None;
        }
        return Some(parse_runtime_pattern_inner(chars));
    }
    None
}

fn take_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, quote: char) -> String {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => break,
            }
        } else if c == quote {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

fn take_while<F: Fn(&char) -> bool>(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    pred: F,
) -> String {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if !pred(&c) {
            break;
        }
        out.push(c);
        chars.next();
    }
    out
}

fn try_match_arm(
    arm: &Value,
    scrutinee: &Value,
    bindings: &mut Vec<(String, Value)>,
    fuel: &mut usize,
) -> Result<Option<Value>, EvalError> {
    // Arm payload is (descriptor, (guard, thunk)).
    let Value::Pair(a, b) = arm else {
        return Err(EvalError::TypeMismatch(format!("malformed match arm: {}", arm.display())));
    };
    let (descriptor, guard, thunk) = match (a.as_ref(), b.as_ref()) {
        (Value::Str(descriptor), Value::Pair(guard, thunk)) => {
            (descriptor.clone(), guard.clone(), thunk.clone())
        }
        _ => {
            return Err(EvalError::TypeMismatch(format!(
                "malformed match arm payload: {}, {}",
                a.display(),
                b.display()
            )));
        }
    };
    let descriptor = parse_runtime_pattern(&descriptor);
    if let Descriptor::Pattern(pattern) = &descriptor
        && !pattern_matches(pattern, scrutinee, bindings)
    {
        bindings.clear();
        return Ok(None);
    }
    if let Value::Closure { param, body, env: closure_env } = guard.as_ref() {
        let mut guard_env = closure_env.clone();
        guard_env.push();
        for (name, value) in bindings.iter() {
            guard_env.define(name.clone(), value.clone());
        }
        guard_env.define(param, Value::Unit);
        if eval(body, &mut guard_env, fuel)? != Value::Bool(true) {
            bindings.clear();
            return Ok(None);
        }
    } else if guard.as_ref() != &Value::Bool(true) {
        bindings.clear();
        return Ok(None);
    }
    let Value::Closure { param, body, env: closure_env } = thunk.as_ref() else {
        return Err(EvalError::TypeMismatch("match arm body must be a thunk".into()));
    };
    let mut call_env = closure_env.clone();
    call_env.push();
    for (name, value) in bindings.iter() {
        call_env.define(name.clone(), value.clone());
    }
    call_env.define(param, scrutinee.clone());
    Ok(Some(eval(body, &mut call_env, fuel)?))
}

fn pattern_matches(
    pattern: &RuntimePattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
) -> bool {
    match pattern {
        RuntimePattern::Wildcard => true,
        RuntimePattern::Binding(name, inner) => {
            if !pattern_matches(inner, value, bindings) {
                return false;
            }
            bindings.push((name.clone(), value.clone()));
            true
        }
        RuntimePattern::Literal(expected) => match (expected, value) {
            // An enum value carries its label; a bare name pattern still
            // matches it by label.
            (Value::Str(name), Value::Tagged(label, _)) => name == label,
            (expected, value) => expected == value,
        },
        RuntimePattern::Tagged(label, fields) => {
            let Value::Tagged(actual, payload) = value else {
                return false;
            };
            if actual != label {
                return false;
            }
            match fields.len() {
                0 => true,
                1 => pattern_matches(&fields[0], payload, bindings),
                _ => {
                    // Several payload values are packed right-nested, so walk
                    // the spine one field at a time.
                    let mut current = payload.as_ref();
                    for (index, field) in fields.iter().enumerate() {
                        if index + 1 == fields.len() {
                            return pattern_matches(field, current, bindings);
                        }
                        let Value::Pair(head, rest) = current else {
                            return false;
                        };
                        if !pattern_matches(field, head, bindings) {
                            return false;
                        }
                        current = rest;
                    }
                    true
                }
            }
        }
        RuntimePattern::Range(start, end) => match (start.as_ref(), end.as_ref(), value) {
            (
                RuntimePattern::Literal(Value::Int(s)),
                RuntimePattern::Literal(Value::Int(e)),
                Value::Int(v),
            ) => v >= s && v <= e,
            (
                RuntimePattern::Literal(Value::Char(s)),
                RuntimePattern::Literal(Value::Char(e)),
                Value::Char(v),
            ) => v >= s && v <= e,
            _ => false,
        },
        RuntimePattern::Or(alternatives) => {
            alternatives.iter().any(|alternative| pattern_matches(alternative, value, bindings))
        }
        RuntimePattern::Tuple(items) => {
            if let Value::Pair(a, b) = value {
                let mut flat = Vec::new();
                collect_args(a, &mut flat);
                collect_args(b, &mut flat);
                flat.len() == items.len()
                    && items
                        .iter()
                        .zip(flat.iter())
                        .all(|(pattern, value)| pattern_matches(pattern, value, bindings))
            } else {
                false
            }
        }
        RuntimePattern::List { items, rest } => {
            let Value::List(list) = value else {
                return false;
            };
            if list.len() < items.len() {
                return false;
            }
            for (pattern, value) in items.iter().zip(list.iter()) {
                if !pattern_matches(pattern, value, bindings) {
                    return false;
                }
            }
            if let Some((name, inner)) = rest {
                let tail = Value::List(list[items.len()..].to_vec());
                if !pattern_matches(inner, &tail, bindings) {
                    return false;
                }
                if let Some(name) = name {
                    bindings.push((name.clone(), tail));
                }
            }
            true
        }
    }
}

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
                // Only the `$`-prefixed encodings are reserved: they cannot
                // be written in the surface, so they never shadow a binding.
                "$unit" => return Ok(Value::Unit),
                "$no_args" => return Ok(Value::NoArguments),
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
            // The marker uses a unique id, independent of evaluation fuel.
            // Activating the binder unwinds to exactly this mu.
            let my_id = fresh_mu_id()?;
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

        Term::Tag(label, payload) => {
            Ok(Value::Tagged(label.clone(), Box::new(eval(payload, env, fuel)?)))
        }

        Term::CoAbs(covar, body) => Ok(Value::CoAbs {
            covar: covar.clone(),
            body: Rc::new((**body).clone()),
            env: env.clone(),
        }),

        Term::Co(coterm) => match coterm.as_ref() {
            // A negative additive consumer closes over its environment. Its
            // branch bodies stay unevaluated: activation runs exactly one.
            CoTerm::CoCase(branches) => {
                Ok(Value::CoCase { branches: Rc::new(branches.clone()), env: env.clone() })
            }
            CoTerm::MuTildeTensor(binders, body) => Ok(Value::CoTensor {
                binders: Rc::new(binders.clone()),
                body: Rc::new((**body).clone()),
                env: env.clone(),
            }),
            // `μ̃x. c` binds the whole value: a product consumer of one part.
            CoTerm::MuTilde(binder, body) => Ok(Value::CoTensor {
                binders: Rc::new(vec![binder.clone()]),
                body: Rc::new((**body).clone()),
                env: env.clone(),
            }),
            other => Ok(Value::Continuation(crate::value::Cont {
                env: env.clone(),
                command: Rc::new(Command::Cut(Term::Var("__co_arg".into()), other.clone())),
            })),
        },
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
                    // ⟨v ∥ α⟩ sends v to α. When α is bound to a continuation
                    // — a consumer parameter, a `select` consumer, or a μ
                    // escape — the cut activates it. A co-variable that only
                    // names the ambient continuation, as the lowering of
                    // `let`, blocks, and applications does, returns the value.
                    match env.lookup(a) {
                        Some(continuation) if is_applicable(&continuation) => {
                            apply_value(continuation, v, fuel)
                        }
                        _ => Ok(v),
                    }
                }
                // ⟨ f ∥ λ̄x. c ⟩ — application: `c` computes the argument.
                CoTerm::CoLam(x, c2) => {
                    let mut env2 = env.clone();
                    env2.push();
                    env2.define(x.clone(), v.clone());
                    {
                        // A builtin decides how many arguments it still needs,
                        // so it collects them itself.
                        if let Value::Builtin(name) = &v {
                            if let Some(id_str) = name.strip_prefix("__mu_escape@")
                                && let Ok(id) = id_str.parse::<u64>()
                            {
                                let arg = eval_argument(c2, &mut env2, fuel)?;
                                return Err(EvalError::Escape(id, arg));
                            }
                            if std::env::var("SLC_DEBUG").is_ok() {
                                eprintln!("[builtin] applying {name}");
                            }
                            return apply_builtin_call(name, c2, &mut env2, fuel);
                        }
                        if let Value::PartialBuiltin(name, mut collected) = v.clone() {
                            if std::env::var("SLC_DEBUG").is_ok() {
                                eprintln!("[partial] {name} has {} args", collected.len());
                            }
                            let new_arg = eval_argument(c2, &mut env2, fuel)?;
                            let mut single = if name == "__match_dispatch" {
                                split_match_payload(&new_arg)
                            } else {
                                Vec::new()
                            };
                            if name != "__match_dispatch" {
                                collect_args(&new_arg, &mut single);
                            }
                            collected.extend(single);
                            let arity = builtin_arity(&name);
                            if collected.len() < arity && name != "__match_dispatch" {
                                return Ok(Value::PartialBuiltin(name, collected));
                            }
                            return dispatch_builtin(&name, collected, fuel);
                        }
                        if is_applicable(&v) {
                            let arg = eval_argument(c2, &mut env2, fuel)?;
                            return apply_value(v, arg, fuel);
                        }
                    }
                    let r = eval_command(c2, &mut env2, fuel)?;
                    Ok(r)
                }
                // ⟨ v ∥ μ̃x. c ⟩ → c[v/x] — a binder: `let`, a discarded
                // block expression, or any other form that names a value.
                CoTerm::MuTilde(x, c2) => {
                    let mut env2 = env.clone();
                    env2.push();
                    env2.define(x.clone(), v);
                    eval_command(c2, &mut env2, fuel)
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
            // k(v): the continuation `k` is an expression, so evaluate it and
            // send it the value. This is the same operation as ⟨v ∥ α⟩ with a
            // bound co-variable, written for a consumer that has no name.
            let continuation = eval(k, env, fuel)?;
            let value = eval(v, env, fuel)?;
            apply_value(continuation, value, fuel)
        }
    }
}

/// The argument of an application: lowering wraps it as ⟨ arg ∥ __call ⟩.
fn eval_argument(
    arg_command: &Command,
    env: &mut Env,
    fuel: &mut usize,
) -> Result<Value, EvalError> {
    match arg_command {
        Command::Cut(t, CoTerm::Covar(_)) => eval(t, env, fuel),
        other => eval_command(other, env, fuel),
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
    if name == "__match_dispatch" {
        args.extend(split_match_payload(&arg));
    } else {
        collect_args(&arg, &mut args);
    }
    // Determine target arity for this builtin.
    let arity = builtin_arity(name);
    if std::env::var("SLC_DEBUG").is_ok() {
        eprintln!("[arity] {name}: got {} args (arity {arity})", args.len());
    }
    if args.len() < arity && name != "__match_dispatch" {
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
        if collected.len() < arity && name != "__match_dispatch" {
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
            let mut call_env = closure_env.clone();
            call_env.push();
            call_env.define(param, Value::Unit);
            return eval(&body, &mut call_env, fuel);
        }
        return Ok(chosen);
    }
    // match dispatch: (scrutinee, arm1_thunk, arm2_thunk, ...)
    if name == "__match_dispatch" {
        let mut it = args.into_iter();
        let scrutinee = it.next().unwrap_or(Value::Unit);
        let mut arms = it.collect::<Vec<_>>();
        // The lowered arm spine ends in unit; that terminator is not an arm.
        if arms.last() == Some(&Value::Unit) {
            arms.pop();
        }
        let mut bindings: Vec<(String, Value)> = Vec::new();
        for arm in arms {
            let arm = unwrap_match_arm(&arm);
            if let Some(v) = try_match_arm(&arm, &scrutinee, &mut bindings, fuel)? {
                return Ok(v);
            }
        }
        return Err(EvalError::TypeMismatch("non-exhaustive match".into()));
    }
    // mu escape: k(v) unwinds to the mu whose binder has this id.
    if let Some(id_str) = name.strip_prefix("__mu_escape@") {
        let id: u64 = id_str
            .parse()
            .map_err(|_| EvalError::TypeMismatch(format!("bad escape marker: {name}")))?;
        let v = args.into_iter().next().unwrap_or(Value::Unit);
        return Err(EvalError::Escape(id, v));
    }
    dispatch_builtin(name, args, fuel)
}

/// Bind the components of a right-nested product to a list of binders. The
/// last binder takes whatever remains, so `n` binders split a product into
/// exactly `n` parts.
fn bind_components(binders: &[String], value: Value, env: &mut Env) -> Result<(), EvalError> {
    let mut rest = value;
    for (index, binder) in binders.iter().enumerate() {
        if index + 1 == binders.len() {
            env.define(binder.clone(), rest);
            return Ok(());
        }
        let Value::Pair(head, tail) = rest else {
            return Err(EvalError::TypeMismatch(format!(
                "a consumer of {} components received {}",
                binders.len(),
                rest.display()
            )));
        };
        env.define(binder.clone(), *head);
        rest = *tail;
    }
    Ok(())
}

/// Can this value receive an argument — as a function or as a continuation?
fn is_applicable(v: &Value) -> bool {
    matches!(
        v,
        Value::Closure { .. }
            | Value::CoCase { .. }
            | Value::CoTensor { .. }
            | Value::CoAbs { .. }
            | Value::Continuation(_)
            | Value::Builtin(_)
    )
}

/// Apply a value to one argument. Ordinary application and continuation
/// activation are the same operation: a cut against something that consumes.
pub fn apply_value(value: Value, arg: Value, fuel: &mut usize) -> Result<Value, EvalError> {
    match value {
        Value::Closure { param, body, env } => {
            let mut call_env = env;
            call_env.push();
            call_env.define(param, arg);
            eval(&body, &mut call_env, fuel)
        }
        // Activating a labelled consumer runs exactly one branch.
        Value::CoCase { branches, env } => {
            let Value::Tagged(label, payload) = arg else {
                return Err(EvalError::TypeMismatch(format!(
                    "activating a `select` consumer requires a labelled value, got {}",
                    arg.display()
                )));
            };
            let Some(branch) = branches.iter().find(|b| b.label == label) else {
                return Err(EvalError::TypeMismatch(format!("no `{label}` alternative in select")));
            };
            let mut branch_env = env;
            branch_env.push();
            bind_components(&branch.binders, *payload, &mut branch_env)?;
            eval_command(&branch.body, &mut branch_env, fuel)
        }
        // Activating a product consumer binds every component.
        Value::CoTensor { binders, body, env } => {
            let mut branch_env = env;
            branch_env.push();
            bind_components(&binders, arg, &mut branch_env)?;
            eval_command(&body, &mut branch_env, fuel)
        }
        // Applying a co-abstraction binds its continuation parameter.
        Value::CoAbs { covar, body, env } => {
            let mut call_env = env;
            call_env.push();
            call_env.define(covar, arg);
            eval(&body, &mut call_env, fuel)
        }
        Value::Continuation(cont) => {
            let mut env = cont.env.clone();
            eval_command(&cont.command, &mut env, fuel)
        }
        Value::Builtin(name) => {
            if let Some(id_str) = name.strip_prefix("__mu_escape@")
                && let Ok(id) = id_str.parse::<u64>()
            {
                return Err(EvalError::Escape(id, arg));
            }
            dispatch_builtin(&name, vec![arg], fuel)
        }
        other => Err(EvalError::TypeMismatch(format!(
            "cannot activate continuation: {}",
            other.display()
        ))),
    }
}

/// A builtin whose outcome is not a single value offers it to continuations
/// instead of returning: the value arguments come first, then one continuation
/// per outcome, and exactly one of them is activated. Returns `None` for a
/// builtin that is an ordinary function.
fn dispatch_offering_builtin(
    name: &str,
    args: &[Value],
    fuel: &mut usize,
) -> Option<Result<Value, EvalError>> {
    let value = |index: usize| args.get(index).cloned().unwrap_or(Value::Unit);
    let message = |text: String| Value::Str(text);
    match name {
        "parse_int" => {
            let (ok, invalid, overflow) = (value(1), value(2), value(3));
            Some(match value(0) {
                Value::Str(text) => match text.parse::<i64>() {
                    Ok(n) => apply_value(ok, Value::Int(n), fuel),
                    Err(e) => {
                        let out_of_range = e.to_string().contains("too large")
                            || e.to_string().contains("too small");
                        let (continuation, reason) = if out_of_range {
                            (overflow, format!("integer out of range: {text:?}"))
                        } else {
                            (invalid, format!("not an integer: {text:?}"))
                        };
                        apply_value(continuation, message(reason), fuel)
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
            Some(match crate::builtins::apply_io_builtin("read_file", &args[..1.min(args.len())]) {
                Ok(contents) => apply_value(ok, contents, fuel),
                Err(e) => apply_value(failed, message(e.to_string()), fuel),
            })
        }
        "open_file" => {
            let (opened, failed) = (value(1), value(2));
            let Value::Str(path) = value(0) else {
                return Some(Err(EvalError::TypeMismatch("open_file expects a String".into())));
            };
            Some(match crate::builtins::open_file(&path) {
                Ok(handle) => apply_value(opened, handle, fuel),
                Err(e) => apply_value(failed, message(e.to_string()), fuel),
            })
        }
        "read_line" => {
            let (line, end) = (value(1), value(2));
            let Value::File(id) = value(0) else {
                return Some(Err(EvalError::TypeMismatch(
                    "read_line expects a file handle".into(),
                )));
            };
            Some(match crate::builtins::read_line(id) {
                Ok(Some(text)) => apply_value(line, Value::Str(text), fuel),
                Ok(None) => apply_value(end, Value::Unit, fuel),
                Err(e) => Err(EvalError::TypeMismatch(e.to_string())),
            })
        }
        "write_file" => {
            let (ok, failed) = (value(2), value(3));
            Some(
                match crate::builtins::apply_io_builtin("write_file", &args[..2.min(args.len())]) {
                    Ok(_) => apply_value(ok, Value::Unit, fuel),
                    Err(e) => apply_value(failed, message(e.to_string()), fuel),
                },
            )
        }
        "char_at" => {
            let (ok, out_of_range) = (value(2), value(3));
            let (Value::Str(text), Value::Int(index)) = (value(0), value(1)) else {
                return Some(Err(EvalError::TypeMismatch("char_at expects (String, i64)".into())));
            };
            Some(match usize::try_from(index).ok().and_then(|i| text.chars().nth(i)) {
                Some(c) => apply_value(ok, Value::Char(c), fuel),
                None => apply_value(
                    out_of_range,
                    message(format!(
                        "index {index} is out of range for a string of length {}",
                        text.chars().count()
                    )),
                    fuel,
                ),
            })
        }
        "list_get" => {
            let (ok, out_of_range) = (value(2), value(3));
            let (Value::List(items), Value::Int(index)) = (value(0), value(1)) else {
                return Some(Err(EvalError::TypeMismatch("list_get expects (list, i64)".into())));
            };
            Some(match usize::try_from(index).ok().and_then(|i| items.get(i).cloned()) {
                Some(item) => apply_value(ok, item, fuel),
                None => apply_value(
                    out_of_range,
                    message(format!(
                        "index {index} is out of range for a list of length {}",
                        items.len()
                    )),
                    fuel,
                ),
            })
        }
        "map_get" => {
            let (found, missing) = (value(2), value(3));
            let (Value::Map(entries), key) = (value(0), value(1)) else {
                return Some(Err(EvalError::TypeMismatch("map_get expects (map, key)".into())));
            };
            Some(match entries.iter().find(|(k, _)| k == &key) {
                Some((_, v)) => apply_value(found, v.clone(), fuel),
                None => {
                    apply_value(missing, message(format!("no entry for {}", key.display())), fuel)
                }
            })
        }
        "find_char" => {
            let (found, absent) = (value(3), value(4));
            let (Value::Str(text), Value::Int(from), Value::Int(target)) =
                (value(0), value(1), value(2))
            else {
                return Some(Err(EvalError::TypeMismatch(
                    "find_char expects (String, i64, i64)".into(),
                )));
            };
            let Some(needle) = u32::try_from(target).ok().and_then(char::from_u32) else {
                return Some(Err(EvalError::TypeMismatch(format!(
                    "find_char expects a character code, got {target}"
                ))));
            };
            let start = usize::try_from(from).unwrap_or(0);
            Some(match text.chars().enumerate().skip(start).find(|(_, c)| *c == needle) {
                Some((index, _)) => apply_value(found, Value::Int(index as i64), fuel),
                None => apply_value(
                    absent,
                    message(format!("{needle:?} does not occur from index {from}")),
                    fuel,
                ),
            })
        }
        _ => None,
    }
}

/// Dispatch a builtin with a complete argument list.
fn dispatch_builtin(name: &str, args: Vec<Value>, _fuel: &mut usize) -> Result<Value, EvalError> {
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
    if let Some(outcome) = dispatch_offering_builtin(name, &args, _fuel) {
        return outcome;
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
        let id: u64 = id_str
            .parse()
            .map_err(|_| EvalError::TypeMismatch(format!("bad escape marker: {name}")))?;
        let v = args.into_iter().next().unwrap_or(Value::Unit);
        return Err(EvalError::Escape(id, v));
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

fn collect_args(v: &Value, out: &mut Vec<Value>) {
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
fn builtin_arity(name: &str) -> usize {
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
        "list_new" => 0,
        "format" => 0, // variadic: apply immediately
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
