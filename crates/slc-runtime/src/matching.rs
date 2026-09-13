//! The runtime pattern engine behind `__match_dispatch`.
//!
//! Lowering encodes each `match` arm's pattern as a printed descriptor;
//! this module parses those descriptors back and matches values against
//! them, binding as it goes.

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RuntimePattern {
    Wildcard,
    Binding(String, Box<RuntimePattern>),
    Literal(Value),
    /// An enum variant pattern: a label and the payload patterns it binds.
    Tagged(String, Vec<RuntimePattern>),
    Range(Box<RuntimePattern>, Box<RuntimePattern>),
    Or(Vec<RuntimePattern>),
    Tuple(Vec<RuntimePattern>),
}

/// A match dispatch payload is one tuple, `(scrutinee, arm₁, arm₂, …)`, each
/// arm itself a tagged `(descriptor, thunk)`.
pub(crate) fn split_match_payload(v: &Value) -> Vec<Value> {
    match v {
        Value::Tuple(items) => items.clone(),
        other => vec![other.clone()],
    }
}

/// Unwrap the `__match_arm` tag lowering wraps each match arm in.
pub(crate) fn unwrap_match_arm(v: &Value) -> Value {
    match v {
        Value::Tagged(label, inner) if label == "__match_arm" => (**inner).clone(),
        other => other.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Descriptor {
    Pattern(RuntimePattern),
    Rest,
}

pub(crate) fn parse_runtime_pattern(s: &str) -> Descriptor {
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

pub(crate) fn pattern_matches(
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
                n => {
                    // Several payload values are one tuple, a field each.
                    let Value::Tuple(items) = payload.as_ref() else { return false };
                    items.len() == n
                        && fields
                            .iter()
                            .zip(items)
                            .all(|(field, item)| pattern_matches(field, item, bindings))
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
        RuntimePattern::Tuple(items) => match value {
            Value::Tuple(values) => {
                values.len() == items.len()
                    && items
                        .iter()
                        .zip(values)
                        .all(|(pattern, value)| pattern_matches(pattern, value, bindings))
            }
            _ => false,
        },
    }
}
