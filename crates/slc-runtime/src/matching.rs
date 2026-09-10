//! The runtime pattern engine behind `__match_dispatch`.
//!
//! Lowering encodes each `match` arm's pattern as a printed descriptor;
//! this module parses those descriptors back and matches values against
//! them, binding as it goes.

use crate::eval::{EvalError, collect_args, eval};
use crate::value::Value;

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
pub(crate) fn split_match_payload(v: &Value) -> Vec<Value> {
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
pub(crate) fn unwrap_match_arm(v: &Value) -> Value {
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

pub(crate) fn try_match_arm(
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
