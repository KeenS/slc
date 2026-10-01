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
    match slc_syntax::pattern::parse_pattern(s) {
        slc_syntax::pattern::Descriptor::Rest => Descriptor::Rest,
        slc_syntax::pattern::Descriptor::Pattern(pat) => Descriptor::Pattern(from_pat(pat)),
    }
}

fn from_pat(pat: slc_syntax::pattern::Pat) -> RuntimePattern {
    use slc_syntax::pattern::Pat;
    match pat {
        Pat::Wildcard => RuntimePattern::Wildcard,
        Pat::Binding(name, inner) => RuntimePattern::Binding(name, Box::new(from_pat(*inner))),
        Pat::Int(n) => RuntimePattern::Literal(Value::Int(n)),
        Pat::Float(n) => RuntimePattern::Literal(Value::Float(n)),
        Pat::Str(text) => RuntimePattern::Literal(Value::Str(text)),
        Pat::Char(c) => RuntimePattern::Literal(Value::Char(c)),
        Pat::Tagged(label, fields) => {
            RuntimePattern::Tagged(label, fields.into_iter().map(from_pat).collect())
        }
        Pat::Range(start, end) => {
            RuntimePattern::Range(Box::new(from_pat(*start)), Box::new(from_pat(*end)))
        }
        Pat::Or(items) => RuntimePattern::Or(items.into_iter().map(from_pat).collect()),
        Pat::Tuple(items) => RuntimePattern::Tuple(items.into_iter().map(from_pat).collect()),
    }
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
            (
                RuntimePattern::Literal(Value::Float(s)),
                RuntimePattern::Literal(Value::Float(e)),
                Value::Float(v),
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
