//! Builtin operations for the standard library.

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinError {
    TypeMismatch(String),
    DivisionByZero,
    ArithmeticOverflow(String),
    UnknownBuiltin(String),
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinError::TypeMismatch(m) => write!(f, "builtin type mismatch: {m}"),
            BuiltinError::DivisionByZero => write!(f, "division by zero"),
            BuiltinError::ArithmeticOverflow(m) => write!(f, "arithmetic overflow: {m}"),
            BuiltinError::UnknownBuiltin(n) => write!(f, "unknown builtin: {n}"),
        }
    }
}

impl std::error::Error for BuiltinError {}

/// Apply a builtin to arguments.
pub fn apply_builtin(
    name: &str,
    args: &[Value],
    out: &mut dyn std::io::Write,
) -> Result<Value, BuiltinError> {
    match name {
        "println" => {
            let s = args.first().map(|v| v.display()).unwrap_or_default();
            writeln!(out, "{s}").map_err(|e| BuiltinError::TypeMismatch(e.to_string()))?;
            Ok(Value::Unit)
        }
        "print" => {
            let s = args.first().map(|v| v.display()).unwrap_or_default();
            write!(out, "{s}").map_err(|e| BuiltinError::TypeMismatch(e.to_string()))?;
            Ok(Value::Unit)
        }
        "neg" => match args.first() {
            Some(Value::Int(n)) => Ok(Value::Int(-n)),
            _ => Err(BuiltinError::TypeMismatch("neg expects an integer argument".into())),
        },
        "add" | "sub" | "mul" | "div" | "rem" => {
            let (a, b) = two_ints(name, args)?;
            let r = match name {
                "add" => a
                    .checked_add(b)
                    .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("add({a}, {b})")))?,
                "sub" => a
                    .checked_sub(b)
                    .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("sub({a}, {b})")))?,
                "mul" => a
                    .checked_mul(b)
                    .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("mul({a}, {b})")))?,
                "div" => {
                    if b == 0 {
                        return Err(BuiltinError::DivisionByZero);
                    }
                    a.wrapping_div(b)
                }
                _ => {
                    if b == 0 {
                        return Err(BuiltinError::DivisionByZero);
                    }
                    a.wrapping_rem(b)
                }
            };
            Ok(Value::Int(r))
        }
        "eq" | "ne" | "lt" | "gt" | "le" | "ge" => {
            let (a, b) = two_ints(name, args)?;
            let r = match name {
                "eq" => a == b,
                "ne" => a != b,
                "lt" => a < b,
                "gt" => a > b,
                "le" => a <= b,
                "ge" => a >= b,
                _ => unreachable!(),
            };
            Ok(Value::Bool(r))
        }
        "str_len" => match args.first() {
            Some(Value::Str(s)) => Ok(Value::Int(s.len() as i64)),
            _ => Err(BuiltinError::TypeMismatch(format!("{name} expects a String argument"))),
        },
        "str_concat" => {
            let (Some(Value::Str(a)), Some(Value::Str(b))) = (args.first(), args.get(1)) else {
                return Err(BuiltinError::TypeMismatch(format!(
                    "{name} expects two String arguments"
                )));
            };
            Ok(Value::Str(format!("{a}{b}")))
        }
        "int_to_str" => match args.first() {
            Some(Value::Int(n)) => Ok(Value::Str(n.to_string())),
            _ => Err(BuiltinError::TypeMismatch(format!("{name} expects an integer argument"))),
        },
        "format" => {
            // Simplified: join all args
            let s = args.iter().map(|v| v.display()).collect::<Vec<_>>().join(" ");
            Ok(Value::Str(s))
        }
        "char_at" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(i))) => {
                let idx = *i as usize;
                s.chars()
                    .nth(idx)
                    .map(|c| Value::Int(c as i64))
                    .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range")))
            }
            _ => Err(BuiltinError::TypeMismatch("char_at expects (String, i64)".into())),
        },
        "is_digit" => match args.first() {
            Some(Value::Int(c)) => Ok(Value::Bool((*c as u8).is_ascii_digit())),
            _ => Err(BuiltinError::TypeMismatch("is_digit expects a char code".into())),
        },
        "is_ws" => match args.first() {
            Some(Value::Int(c)) => Ok(Value::Bool(
                *c == ' ' as i64 || *c == '\n' as i64 || *c == '\r' as i64 || *c == '\t' as i64,
            )),
            _ => Err(BuiltinError::TypeMismatch("is_ws expects a char code".into())),
        },
        "skip_digits" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(pos))) => {
                let mut i = *pos as usize;
                let chars: Vec<char> = s.chars().collect();
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                Ok(Value::Int(i as i64))
            }
            _ => Err(BuiltinError::TypeMismatch("skip_digits expects (String, i64)".into())),
        },
        "find_char" => match (args.first(), args.get(1), args.get(2)) {
            (Some(Value::Str(s)), Some(Value::Int(from)), Some(Value::Int(target))) => {
                let start = *from as usize;
                let chars: Vec<char> = s.chars().collect();
                let tc = char::from_u32(*target as u32)
                    .ok_or_else(|| BuiltinError::TypeMismatch("bad char code".into()))?;
                for (i, c) in chars.iter().enumerate().skip(start) {
                    if *c == tc {
                        return Ok(Value::Int(i as i64));
                    }
                }
                Ok(Value::Int(-1))
            }
            _ => Err(BuiltinError::TypeMismatch("find_char expects (String, i64, i64)".into())),
        },
        "skip_ws" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(pos))) => {
                let mut i = *pos as usize;
                let chars: Vec<char> = s.chars().collect();
                while i < chars.len() && {
                    let c = chars[i];
                    c == ' ' || c == '\n' || c == '\r' || c == '\t'
                } {
                    i += 1;
                }
                Ok(Value::Int(i as i64))
            }
            _ => Err(BuiltinError::TypeMismatch("skip_ws expects (String, i64)".into())),
        },
        "substring" => {
            if std::env::var("SLC_DEBUG").is_ok() {
                eprintln!(
                    "[substring] args: {:?}",
                    args.iter().map(|v| v.display()).collect::<Vec<_>>()
                );
            }
            match (args.first(), args.get(1), args.get(2)) {
                (Some(Value::Str(s)), Some(Value::Int(start)), Some(Value::Int(end))) => {
                    let a = (*start).max(0) as usize;
                    let b = (*end).max(0) as usize;
                    if b > a && b <= s.len() {
                        Ok(Value::Str(s[a..b].to_string()))
                    } else {
                        Ok(Value::Str(String::new()))
                    }
                }
                _ => Err(BuiltinError::TypeMismatch("substring expects (String, i64, i64)".into())),
            }
        }
        "str_to_int" => match args.first() {
            Some(Value::Str(s)) => s
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| BuiltinError::TypeMismatch(format!("cannot parse {s:?} as integer"))),
            _ => Err(BuiltinError::TypeMismatch("str_to_int expects a String".into())),
        },
        "str_eq" => match (args.first(), args.get(1)) {
            (Some(Value::Str(a)), Some(Value::Str(b))) => Ok(Value::Bool(a == b)),
            _ => Err(BuiltinError::TypeMismatch("str_eq expects two Strings".into())),
        },
        "list_len" => match args.first() {
            Some(Value::List(items)) => Ok(Value::Int(items.len() as i64)),
            _ => Err(BuiltinError::TypeMismatch("list_len expects a List".into())),
        },
        "list_push" => match (args.first(), args.get(1)) {
            (Some(Value::List(items)), Some(v)) => {
                let mut new_items = items.clone();
                new_items.push(v.clone());
                Ok(Value::List(new_items))
            }
            _ => Err(BuiltinError::TypeMismatch("list_push expects (List, value)".into())),
        },
        "list_get" => match (args.first(), args.get(1)) {
            (Some(Value::List(items)), Some(Value::Int(i))) => items
                .get(*i as usize)
                .cloned()
                .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range"))),
            _ => Err(BuiltinError::TypeMismatch("list_get expects (List, i64)".into())),
        },
        other => Err(BuiltinError::UnknownBuiltin(other.to_string())),
    }
}

fn two_ints(name: &str, args: &[Value]) -> Result<(i64, i64), BuiltinError> {
    match (args.first(), args.get(1)) {
        (Some(Value::Int(a)), Some(Value::Int(b))) => Ok((*a, *b)),
        _ => Err(BuiltinError::TypeMismatch(format!("{name} expects two integer arguments"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn println_outputs() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("println", &[Value::Int(42)], &mut buf).unwrap();
        assert_eq!(r, Value::Unit);
        assert_eq!(String::from_utf8(buf).unwrap(), "42\n");
    }

    #[test]
    fn add_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("add", &[Value::Int(1), Value::Int(2)], &mut buf).unwrap();
        assert_eq!(r, Value::Int(3));
    }

    #[test]
    fn div_by_zero() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("div", &[Value::Int(1), Value::Int(0)], &mut buf);
        assert_eq!(r, Err(BuiltinError::DivisionByZero));
    }

    #[test]
    fn eq_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("eq", &[Value::Int(1), Value::Int(1)], &mut buf).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn str_len_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("str_len", &[Value::Str("hello".into())], &mut buf).unwrap();
        assert_eq!(r, Value::Int(5));
    }

    #[test]
    fn str_concat_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin(
            "str_concat",
            &[Value::Str("foo".into()), Value::Str("bar".into())],
            &mut buf,
        )
        .unwrap();
        assert_eq!(r, Value::Str("foobar".into()));
    }

    #[test]
    fn int_to_str_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("int_to_str", &[Value::Int(123)], &mut buf).unwrap();
        assert_eq!(r, Value::Str("123".into()));
    }

    #[test]
    fn unknown_builtin() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("nope", &[], &mut buf);
        assert!(matches!(r, Err(BuiltinError::UnknownBuiltin(_))));
    }

    #[test]
    fn overflow_detected() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("mul", &[Value::Int(i64::MAX), Value::Int(2)], &mut buf);
        assert!(matches!(r, Err(BuiltinError::ArithmeticOverflow(_))));
    }

    #[test]
    fn list_operations() {
        let mut buf: Vec<u8> = Vec::new();
        let empty = Value::List(vec![]);
        let l1 = apply_builtin("list_push", &[empty, Value::Int(1)], &mut buf).unwrap();
        assert_eq!(l1, Value::List(vec![Value::Int(1)]));
        let l2 = apply_builtin("list_push", &[l1, Value::Int(2)], &mut buf).unwrap();
        assert_eq!(l2, Value::List(vec![Value::Int(1), Value::Int(2)]));
        let n = apply_builtin("list_len", &[l2.clone()], &mut buf).unwrap();
        assert_eq!(n, Value::Int(2));
        let item = apply_builtin("list_get", &[l2, Value::Int(1)], &mut buf).unwrap();
        assert_eq!(item, Value::Int(2));
    }
}

/// Install file I/O builtins (separate from pure builtins for clarity).
pub fn apply_io_builtin(name: &str, args: &[Value]) -> Result<Value, BuiltinError> {
    match name {
        "read_file" => {
            let Some(Value::Str(path)) = args.first() else {
                return Err(BuiltinError::TypeMismatch("read_file expects a String path".into()));
            };
            match std::fs::read_to_string(path) {
                Ok(s) => Ok(Value::Str(s)),
                Err(e) => Err(BuiltinError::TypeMismatch(format!("cannot read {path}: {e}"))),
            }
        }
        "write_file" => {
            let (Some(Value::Str(path)), Some(Value::Str(content))) = (args.first(), args.get(1))
            else {
                return Err(BuiltinError::TypeMismatch(
                    "write_file expects (path, content) Strings".into(),
                ));
            };
            std::fs::write(path, content)
                .map_err(|e| BuiltinError::TypeMismatch(format!("cannot write {path}: {e}")))?;
            Ok(Value::Unit)
        }
        "file_exists" => {
            let Some(Value::Str(path)) = args.first() else {
                return Err(BuiltinError::TypeMismatch("file_exists expects a String path".into()));
            };
            Ok(Value::Bool(std::path::Path::new(path).exists()))
        }
        other => Err(BuiltinError::UnknownBuiltin(other.to_string())),
    }
}

#[cfg(test)]
mod io_tests {
    use super::*;

    #[test]
    fn write_and_read_file() {
        let path = std::env::temp_dir().join("slc_io_test.txt");
        let p = path.to_str().unwrap();
        apply_io_builtin("write_file", &[Value::Str(p.into()), Value::Str("data".into())]).unwrap();
        let r = apply_io_builtin("read_file", &[Value::Str(p.into())]).unwrap();
        assert_eq!(r, Value::Str("data".into()));
    }

    #[test]
    fn file_exists() {
        let path = std::env::temp_dir().join("slc_io_exists_test.txt");
        let p = path.to_str().unwrap();
        std::fs::write(&path, "x").unwrap();
        let r = apply_io_builtin("file_exists", &[Value::Str(p.into())]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn read_missing_file_fails() {
        let r = apply_io_builtin("read_file", &[Value::Str("/nonexistent/nope".into())]);
        assert!(matches!(r, Err(BuiltinError::TypeMismatch(_))));
    }
}
