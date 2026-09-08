//! Builtin operations for the standard library.

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinError {
    TypeMismatch(String),
    DivisionByZero,
    UnknownBuiltin(String),
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinError::TypeMismatch(m) => write!(f, "builtin type mismatch: {m}"),
            BuiltinError::DivisionByZero => write!(f, "division by zero"),
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
        "add" | "sub" | "mul" | "div" | "rem" => {
            let (a, b) = two_ints(name, args)?;
            let r = match name {
                "add" => a.wrapping_add(b),
                "sub" => a.wrapping_sub(b),
                "mul" => a.wrapping_mul(b),
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
