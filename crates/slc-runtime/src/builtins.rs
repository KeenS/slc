//! Builtin operations for the standard library.

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinError {
    TypeMismatch(String),
    /// An outcome, not a fault: the operation could not be carried out. It is
    /// reported to a failure continuation, so it carries only the message.
    Failed(String),
    DivisionByZero,
    ArithmeticOverflow(String),
    UnknownBuiltin(String),
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinError::TypeMismatch(m) => write!(f, "builtin type mismatch: {m}"),
            BuiltinError::Failed(m) => write!(f, "{m}"),
            BuiltinError::DivisionByZero => write!(f, "division by zero"),
            BuiltinError::ArithmeticOverflow(m) => write!(f, "arithmetic overflow: {m}"),
            BuiltinError::UnknownBuiltin(n) => write!(f, "unknown builtin: {n}"),
        }
    }
}

impl std::error::Error for BuiltinError {}

fn cmp_op<T: PartialOrd>(name: &str, a: T, b: T) -> bool {
    match name {
        "eq" => a == b,
        "ne" => a != b,
        "lt" => a < b,
        "gt" => a > b,
        "le" => a <= b,
        "ge" => a >= b,
        _ => false,
    }
}

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
            if name == "add"
                && let (Some(Value::Str(a)), Some(Value::Str(b))) = (args.first(), args.get(1))
            {
                return Ok(Value::Str(format!("{a}{b}")));
            }
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
            let r = match (args.first(), args.get(1)) {
                (Some(Value::Int(a)), Some(Value::Int(b))) => Some(cmp_op(name, *a, *b)),
                (Some(Value::Char(a)), Some(Value::Char(b))) => Some(cmp_op(name, *a, *b)),
                (Some(Value::Str(a)), Some(Value::Str(b))) => Some(cmp_op(name, a, b)),
                (Some(Value::Bool(a)), Some(Value::Bool(b))) => Some(cmp_op(name, *a, *b)),
                _ => None,
            };
            let r = r.ok_or_else(|| {
                BuiltinError::TypeMismatch(format!(
                    "{name} expects two matching integer, char, String, or bool arguments"
                ))
            })?;
            Ok(Value::Bool(r))
        }
        "str_len" => match args.first() {
            Some(Value::Str(s)) => Ok(Value::Int(s.chars().count() as i64)),
            _ => Err(BuiltinError::TypeMismatch(format!("{name} expects a String argument"))),
        },
        "str_len_bytes" => match args.first() {
            Some(Value::Str(s)) => Ok(Value::Int(s.len() as i64)),
            _ => Err(BuiltinError::TypeMismatch(format!("{name} expects a String argument"))),
        },
        "char_at" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(i))) => {
                let idx = *i as usize;
                s.chars()
                    .nth(idx)
                    .map(Value::Char)
                    .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range")))
            }
            _ => Err(BuiltinError::TypeMismatch("char_at expects (String, i64)".into())),
        },
        "list_new" => Ok(Value::List(Vec::new())),
        "__index" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(i))) => {
                let idx = *i as usize;
                s.chars()
                    .nth(idx)
                    .map(Value::Char)
                    .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range")))
            }
            (Some(Value::List(items)), Some(Value::Int(i))) => items
                .get(*i as usize)
                .cloned()
                .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range"))),
            _ => Err(BuiltinError::TypeMismatch("__index expects (String|List, i64)".into())),
        },
        "char_code_at" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(i))) => {
                let idx = *i as usize;
                s.chars()
                    .nth(idx)
                    .map(|c| Value::Int(c as i64))
                    .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range")))
            }
            _ => Err(BuiltinError::TypeMismatch("char_code_at expects (String, i64)".into())),
        },
        "char_to_code" => match args.first() {
            Some(Value::Char(c)) => Ok(Value::Int(*c as i64)),
            _ => Err(BuiltinError::TypeMismatch("char_to_code expects a char".into())),
        },
        "code_to_char" => match args.first() {
            Some(Value::Int(n)) => u32::try_from(*n)
                .ok()
                .and_then(char::from_u32)
                .map(Value::Char)
                .ok_or_else(|| BuiltinError::TypeMismatch(format!("invalid char code {n}"))),
            _ => Err(BuiltinError::TypeMismatch("code_to_char expects an i64".into())),
        },
        "string_push" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Char(c))) => {
                let mut s = s.clone();
                s.push(*c);
                Ok(Value::Str(s))
            }
            _ => Err(BuiltinError::TypeMismatch("string_push expects (String, char)".into())),
        },
        "is_digit" => match args.first() {
            Some(Value::Char(c)) => Ok(Value::Bool(c.is_ascii_digit())),
            _ => Err(BuiltinError::TypeMismatch("is_digit expects a char".into())),
        },
        "is_ws" => match args.first() {
            Some(Value::Char(c)) => Ok(Value::Bool(c.is_whitespace())),
            _ => Err(BuiltinError::TypeMismatch("is_ws expects a char".into())),
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
                    let chars: Vec<char> = s.chars().collect();
                    let a = (*start).max(0) as usize;
                    let b = (*end).max(0) as usize;
                    if a > b || b > chars.len() {
                        Err(BuiltinError::TypeMismatch(format!(
                            "slice range {start}..{end} out of bounds for length {}",
                            chars.len()
                        )))
                    } else {
                        Ok(Value::Str(chars[a..b].iter().collect()))
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
        "map_new" => Ok(Value::Map(vec![])),
        "map_insert" => match (args.first(), args.get(1), args.get(2)) {
            (Some(Value::Map(entries)), Some(k), Some(v)) => {
                let mut new_entries = entries.clone();
                if let Some(entry) = new_entries.iter_mut().find(|(ek, _)| ek == k) {
                    entry.1 = v.clone();
                } else {
                    new_entries.push((k.clone(), v.clone()));
                }
                Ok(Value::Map(new_entries))
            }
            _ => Err(BuiltinError::TypeMismatch("map_insert expects (Map, key, value)".into())),
        },
        "map_get" => match (args.first(), args.get(1)) {
            (Some(Value::Map(entries)), Some(k)) => Ok(entries
                .iter()
                .find(|(ek, _)| ek == k)
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Unit)),
            _ => Err(BuiltinError::TypeMismatch("map_get expects (Map, key)".into())),
        },
        "map_len" => match args.first() {
            Some(Value::Map(entries)) => Ok(Value::Int(entries.len() as i64)),
            _ => Err(BuiltinError::TypeMismatch("map_len expects a Map".into())),
        },
        "path_join" => match (args.first(), args.get(1)) {
            (Some(Value::Str(a)), Some(Value::Str(b))) => {
                let joined = std::path::Path::new(a).join(b);
                Ok(Value::Str(joined.to_string_lossy().into_owned()))
            }
            _ => Err(BuiltinError::TypeMismatch("path_join expects two Strings".into())),
        },
        "path_basename" => match args.first() {
            Some(Value::Str(p)) => Ok(Value::Str(
                std::path::Path::new(p)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )),
            _ => Err(BuiltinError::TypeMismatch("path_basename expects a String".into())),
        },
        "path_dirname" => match args.first() {
            Some(Value::Str(p)) => Ok(Value::Str(
                std::path::Path::new(p)
                    .parent()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )),
            _ => Err(BuiltinError::TypeMismatch("path_dirname expects a String".into())),
        },
        "path_extension" => match args.first() {
            Some(Value::Str(p)) => Ok(Value::Str(
                std::path::Path::new(p)
                    .extension()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )),
            _ => Err(BuiltinError::TypeMismatch("path_extension expects a String".into())),
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
        let n = apply_builtin("list_len", std::slice::from_ref(&l2), &mut buf).unwrap();
        assert_eq!(n, Value::Int(2));
        let item = apply_builtin("list_get", &[l2, Value::Int(1)], &mut buf).unwrap();
        assert_eq!(item, Value::Int(2));
    }

    #[test]
    fn map_operations() {
        let mut buf: Vec<u8> = Vec::new();
        let m0 = apply_builtin("map_new", &[], &mut buf).unwrap();
        let m1 =
            apply_builtin("map_insert", &[m0, Value::Str("a".into()), Value::Int(1)], &mut buf)
                .unwrap();
        let m2 =
            apply_builtin("map_insert", &[m1, Value::Str("b".into()), Value::Int(2)], &mut buf)
                .unwrap();
        let v = apply_builtin("map_get", &[m2.clone(), Value::Str("a".into())], &mut buf).unwrap();
        assert_eq!(v, Value::Int(1));
        let n = apply_builtin("map_len", &[m2], &mut buf).unwrap();
        assert_eq!(n, Value::Int(2));
    }

    #[test]
    fn path_operations() {
        let mut buf: Vec<u8> = Vec::new();
        let joined = apply_builtin(
            "path_join",
            &[Value::Str("a/b".into()), Value::Str("c.sl".into())],
            &mut buf,
        )
        .unwrap();
        assert_eq!(joined, Value::Str("a/b/c.sl".into()));
        let base =
            apply_builtin("path_basename", &[Value::Str("a/b/c.sl".into())], &mut buf).unwrap();
        assert_eq!(base, Value::Str("c.sl".into()));
        let dir =
            apply_builtin("path_dirname", &[Value::Str("a/b/c.sl".into())], &mut buf).unwrap();
        assert_eq!(dir, Value::Str("a/b".into()));
        let ext =
            apply_builtin("path_extension", &[Value::Str("a/b/c.sl".into())], &mut buf).unwrap();
        assert_eq!(ext, Value::Str("sl".into()));
    }
}

thread_local! {
    /// Open file handles, by id. A handle value is just the id; the reader
    /// lives here until `close_file` spends it.
    static OPEN_FILES: std::cell::RefCell<
        std::collections::HashMap<u64, std::io::BufReader<std::fs::File>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
    static NEXT_FILE_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

/// Open a file for reading and register its handle.
pub fn open_file(path: &str) -> Result<Value, BuiltinError> {
    let file = std::fs::File::open(path)
        .map_err(|e| BuiltinError::Failed(format!("cannot open {path}: {e}")))?;
    let id = NEXT_FILE_ID.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    OPEN_FILES.with(|files| files.borrow_mut().insert(id, std::io::BufReader::new(file)));
    Ok(Value::File(id))
}

/// Read one line from a handle: `Some(line)` without its newline, or `None`
/// at the end of the file. A handle that was never opened — or already
/// closed — is a type error, not an outcome.
pub fn read_line(id: u64) -> Result<Option<String>, BuiltinError> {
    OPEN_FILES.with(|files| {
        let mut files = files.borrow_mut();
        let reader = files
            .get_mut(&id)
            .ok_or_else(|| BuiltinError::Failed(format!("file handle {id} is not open")))?;
        let mut line = String::new();
        use std::io::BufRead;
        let read = reader
            .read_line(&mut line)
            .map_err(|e| BuiltinError::Failed(format!("cannot read from handle {id}: {e}")))?;
        if read == 0 {
            return Ok(None);
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(Some(line))
    })
}

/// Close a handle: spend it, so a later read through it fails.
pub fn close_file(id: u64) -> Result<Value, BuiltinError> {
    OPEN_FILES.with(|files| match files.borrow_mut().remove(&id) {
        Some(_) => Ok(Value::Unit),
        None => Err(BuiltinError::Failed(format!("file handle {id} is not open"))),
    })
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
                Err(e) => Err(BuiltinError::Failed(format!("cannot read {path}: {e}"))),
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
                .map_err(|e| BuiltinError::Failed(format!("cannot write {path}: {e}")))?;
            Ok(Value::Unit)
        }
        "close_file" => {
            let Some(Value::File(id)) = args.first() else {
                return Err(BuiltinError::TypeMismatch("close_file expects a file handle".into()));
            };
            close_file(*id)
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
        // Reading a missing file is an outcome the caller handles, not a
        // fault in the program.
        assert!(matches!(r, Err(BuiltinError::Failed(_))), "{r:?}");
    }
}
