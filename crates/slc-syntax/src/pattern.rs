//! Parser for the descriptor strings [`pattern_descriptor`] writes.
//!
//! The interpreter and the native compiler both read this grammar. A second
//! copy would drift from the printer in `lower.rs`.
//!
//! [`pattern_descriptor`]: crate::lower

/// One pattern in a descriptor. Literals are scalars, not runtime values, so
/// this crate does not depend on the interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum Pat {
    Wildcard,
    Binding(String, Box<Pat>),
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Tagged(String, Vec<Pat>),
    Range(Box<Pat>, Box<Pat>),
    Or(Vec<Pat>),
    Tuple(Vec<Pat>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Descriptor {
    Pattern(Pat),
    Rest,
}

pub fn parse_pattern(s: &str) -> Descriptor {
    if s == ".." {
        return Descriptor::Rest;
    }
    let mut chars = s.chars().peekable();
    Descriptor::Pattern(parse_inner(&mut chars))
}

fn parse_inner(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Pat {
    match chars.next() {
        Some('*') => Pat::Wildcard,
        Some('.') if chars.peek() == Some(&'.') => {
            chars.next();
            Pat::Wildcard
        }
        Some('#') => {
            let text = take_number(chars);
            let rest = parse_tail(chars);
            match (text.parse::<i64>().ok(), text.parse::<f64>().ok(), rest) {
                (Some(start), _, Some(Pat::Int(end))) => {
                    Pat::Range(Box::new(Pat::Int(start)), Box::new(Pat::Int(end)))
                }
                (Some(n), _, _) => Pat::Int(n),
                (None, Some(n), _) => Pat::Float(n),
                _ => Pat::Wildcard,
            }
        }
        Some('%') => {
            let text = take_number(chars);
            let rest = parse_tail(chars);
            match (text.parse::<f64>().ok(), rest) {
                (Some(start), Some(Pat::Float(end))) => {
                    Pat::Range(Box::new(Pat::Float(start)), Box::new(Pat::Float(end)))
                }
                (Some(n), _) => Pat::Float(n),
                _ => Pat::Wildcard,
            }
        }
        Some('"') => {
            let text = take_quoted(chars, '"');
            // A quoted name followed by `(` is a variant, not a string literal.
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
                    fields.push(parse_inner(chars));
                    if chars.next() != Some(',') {
                        break;
                    }
                }
                return Pat::Tagged(text, fields);
            }
            Pat::Str(text)
        }
        Some('\'') => {
            let c = match chars.next() {
                Some('\\') => chars.next().unwrap_or('\\'),
                Some(c) => c,
                None => '\0',
            };
            let _ = chars.next();
            let rest = parse_tail(chars);
            match rest {
                Some(Pat::Char(end_c)) => {
                    Pat::Range(Box::new(Pat::Char(c)), Box::new(Pat::Char(end_c)))
                }
                _ => Pat::Char(c),
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
                items.push(parse_inner(chars));
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
                            items.push(parse_inner(chars));
                            if chars.next() != Some('|') {
                                break;
                            }
                        }
                        return Pat::Or(items);
                    }
                    Some(',') => {}
                    _ => break,
                }
            }
            Pat::Tuple(items)
        }
        Some('$') => Pat::Binding(
            take_while(chars, |c| c.is_alphanumeric() || *c == '_'),
            Box::new(Pat::Wildcard),
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
                    return Pat::Binding(name, Box::new(Pat::Wildcard));
                }
                return Pat::Binding(name, Box::new(parse_inner(chars)));
            }
            Pat::Binding(name, Box::new(Pat::Wildcard))
        }
        None => Pat::Wildcard,
    }
}

fn parse_tail(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<Pat> {
    if chars.peek() == Some(&'.') {
        chars.next();
        if chars.next() != Some('.') {
            return None;
        }
        if chars.next() != Some('=') {
            return None;
        }
        return Some(parse_inner(chars));
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

/// One numeric endpoint, leaving `..` for the range parser.
fn take_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if c == '.' && chars.clone().nth(1) == Some('.') {
            break;
        }
        if c.is_ascii_digit() || c == '-' || c == '.' {
            out.push(c);
            chars.next();
        } else {
            break;
        }
    }
    out
}
