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

/// The inclusive range of a width-conversion builtin. `u64` stops at
/// `i64::MAX`: the machine word is signed, and a larger magnitude has
/// nowhere to sit.
fn integer_destination(name: &str) -> Option<(&'static str, i64, i64)> {
    Some(match name {
        "__to_i8" => ("i8", i64::from(i8::MIN), i64::from(i8::MAX)),
        "__to_i32" => ("i32", i64::from(i32::MIN), i64::from(i32::MAX)),
        "__to_i64" => ("i64", i64::MIN, i64::MAX),
        "__to_u8" => ("u8", 0, i64::from(u8::MAX)),
        "__to_u32" => ("u32", 0, i64::from(u32::MAX)),
        "__to_u64" => ("u64", 0, i64::MAX),
        _ => return None,
    })
}

fn cmp_op<T: PartialOrd>(name: &str, a: T, b: T) -> bool {
    match name {
        "__eq" => a == b,
        "__ne" => a != b,
        "__lt" => a < b,
        "__gt" => a > b,
        "__le" => a <= b,
        "__ge" => a >= b,
        _ => false,
    }
}

/// Apply a builtin to arguments.
pub fn apply_builtin(
    name: &str,
    args: &[Value],
    _out: &mut dyn std::io::Write,
) -> Result<Value, BuiltinError> {
    match name {
        // A base value as the text a person reads: a string or a character is
        // itself, unquoted. The prelude's `Display` impls rest on it.
        "__display" => Ok(Value::Str(match args.first() {
            Some(Value::Str(s)) => s.clone(),
            Some(Value::Char(c)) => c.to_string(),
            Some(v) => v.display(),
            None => String::new(),
        })),
        // Beneath `Into`. Every integer is one signed word, so the check is
        // the destination's range. A `u64` reaches as far as `i64` does.
        "__to_i8" | "__to_i32" | "__to_i64" | "__to_u8" | "__to_u32" | "__to_u64" => {
            let Some(Value::Int(n)) = args.first() else {
                return Err(BuiltinError::TypeMismatch(format!(
                    "{name} expects an integer argument"
                )));
            };
            let (width, lo, hi) =
                integer_destination(name).expect("a width builtin names its range");
            if *n < lo || *n > hi {
                return Err(BuiltinError::ArithmeticOverflow(format!(
                    "{n} does not fit in {width}"
                )));
            }
            Ok(Value::Int(*n))
        }
        "__neg" => match args.first() {
            // `-n` panics in debug when `n` is `i64::MIN`.
            Some(Value::Int(n)) => n
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("neg({n})"))),
            Some(Value::Float(n)) => Ok(Value::Float(-n)),
            _ => Err(BuiltinError::TypeMismatch("neg expects an integer or float argument".into())),
        },
        "__add" | "__sub" | "__mul" | "__div" | "__rem" => {
            if name == "__add"
                && let (Some(Value::Str(a)), Some(Value::Str(b))) = (args.first(), args.get(1))
            {
                return Ok(Value::Str(format!("{a}{b}")));
            }
            if let (Some(Value::Float(a)), Some(Value::Float(b))) = (args.first(), args.get(1)) {
                let r = match name {
                    "__add" => a + b,
                    "__sub" => a - b,
                    "__mul" => a * b,
                    "__div" => a / b,
                    _ => a % b,
                };
                return Ok(Value::Float(r));
            }
            let (a, b) = two_ints(name, args)?;
            let r = match name {
                "__add" => a
                    .checked_add(b)
                    .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("add({a}, {b})")))?,
                "__sub" => a
                    .checked_sub(b)
                    .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("sub({a}, {b})")))?,
                "__mul" => a
                    .checked_mul(b)
                    .ok_or_else(|| BuiltinError::ArithmeticOverflow(format!("mul({a}, {b})")))?,
                "__div" => {
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
        "__eq" | "__ne" | "__lt" | "__gt" | "__le" | "__ge" => {
            let r = match (args.first(), args.get(1)) {
                (Some(Value::Int(a)), Some(Value::Int(b))) => Some(cmp_op(name, *a, *b)),
                (Some(Value::Float(a)), Some(Value::Float(b))) => Some(cmp_op(name, *a, *b)),
                (Some(Value::Char(a)), Some(Value::Char(b))) => Some(cmp_op(name, *a, *b)),
                (Some(Value::Str(a)), Some(Value::Str(b))) => Some(cmp_op(name, a, b)),
                (Some(a), Some(b)) => match (crate::value::as_bool(a), crate::value::as_bool(b)) {
                    (Some(a), Some(b)) => Some(cmp_op(name, a, b)),
                    _ => None,
                },
                _ => None,
            };
            let r = r.ok_or_else(|| {
                BuiltinError::TypeMismatch(format!(
                    "{name} expects two matching numeric, char, String, or Bool arguments"
                ))
            })?;
            Ok(crate::value::bool_value(r))
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
        "__index" => match (args.first(), args.get(1)) {
            (Some(Value::Str(s)), Some(Value::Int(i))) => {
                let idx = *i as usize;
                s.chars()
                    .nth(idx)
                    .map(Value::Char)
                    .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {i} out of range")))
            }
            _ => Err(BuiltinError::TypeMismatch("__index expects (String, i64)".into())),
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
            Some(Value::Char(c)) => Ok(crate::value::bool_value(c.is_ascii_digit())),
            _ => Err(BuiltinError::TypeMismatch("is_digit expects a char".into())),
        },
        "is_ws" => match args.first() {
            Some(Value::Char(c)) => Ok(crate::value::bool_value(c.is_whitespace())),
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
            (Some(Value::Str(a)), Some(Value::Str(b))) => Ok(crate::value::bool_value(a == b)),
            _ => Err(BuiltinError::TypeMismatch("str_eq expects two Strings".into())),
        },
        // The machine word's product in the ring of 64-bit patterns. Checked
        // `mul` refuses the overflow this wraps.
        "__wrapping_mul" => {
            let (a, b) = two_ints(name, args)?;
            Ok(Value::Int(a.wrapping_mul(b)))
        }
        "__xor" => {
            let (a, b) = two_ints(name, args)?;
            Ok(Value::Int(a ^ b))
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
    fn display_renders_unquoted() {
        let mut buf: Vec<u8> = Vec::new();
        for (value, text) in
            [(Value::Int(42), "42"), (Value::Str("hi".into()), "hi"), (Value::Char('c'), "c")]
        {
            let r = apply_builtin("__display", &[value], &mut buf).unwrap();
            assert_eq!(r, Value::Str(text.into()));
        }
    }

    #[test]
    fn wrapping_mul_keeps_the_low_bits_and_xor_mixes_them() {
        let mut buf = Vec::new();
        let product =
            apply_builtin("__wrapping_mul", &[Value::Int(i64::MAX), Value::Int(2)], &mut buf)
                .unwrap();
        assert_eq!(product, Value::Int(i64::MAX.wrapping_mul(2)));
        let mixed = apply_builtin("__xor", &[Value::Int(-1), Value::Int(0x41)], &mut buf).unwrap();
        assert_eq!(mixed, Value::Int(-1 ^ 0x41));
        let code = apply_builtin("char_to_code", &[Value::Char('a')], &mut buf).unwrap();
        assert_eq!(code, Value::Int('a' as i64));
    }

    #[test]
    fn add_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("__add", &[Value::Int(1), Value::Int(2)], &mut buf).unwrap();
        assert_eq!(r, Value::Int(3));
    }

    #[test]
    fn div_by_zero() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("__div", &[Value::Int(1), Value::Int(0)], &mut buf);
        assert_eq!(r, Err(BuiltinError::DivisionByZero));
    }

    #[test]
    fn eq_works() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("__eq", &[Value::Int(1), Value::Int(1)], &mut buf).unwrap();
        assert_eq!(r, crate::value::bool_value(true));
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
        let r = apply_builtin("__mul", &[Value::Int(i64::MAX), Value::Int(2)], &mut buf);
        assert!(matches!(r, Err(BuiltinError::ArithmeticOverflow(_))));
    }

    #[test]
    fn neg_of_i64_min_overflows() {
        let mut buf: Vec<u8> = Vec::new();
        let r = apply_builtin("__neg", &[Value::Int(i64::MIN)], &mut buf);
        assert!(matches!(
            r,
            Err(BuiltinError::ArithmeticOverflow(m)) if m == format!("neg({})", i64::MIN)
        ));
    }

    #[test]
    fn a_width_conversion_keeps_a_value_that_fits_and_refuses_one_that_does_not() {
        let mut buf: Vec<u8> = Vec::new();
        let kept = apply_builtin("__to_i32", &[Value::Int(40_000)], &mut buf).unwrap();
        assert_eq!(kept, Value::Int(40_000));
        let widened = apply_builtin("__to_i64", &[Value::Int(-3)], &mut buf).unwrap();
        assert_eq!(widened, Value::Int(-3));
        let narrow = apply_builtin("__to_i8", &[Value::Int(200)], &mut buf);
        assert!(matches!(narrow, Err(BuiltinError::ArithmeticOverflow(m)) if m.contains("i8")));
        let negative = apply_builtin("__to_u64", &[Value::Int(-1)], &mut buf);
        assert!(matches!(negative, Err(BuiltinError::ArithmeticOverflow(m)) if m.contains("u64")));
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
        "__read_file" => {
            let Some(Value::Str(path)) = args.first() else {
                return Err(BuiltinError::TypeMismatch("read_file expects a String path".into()));
            };
            match std::fs::read_to_string(path) {
                Ok(s) => Ok(Value::Str(s)),
                Err(e) => Err(BuiltinError::Failed(format!("cannot read {path}: {e}"))),
            }
        }
        "__write_file" => {
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
        "__close_file" => {
            let Some(Value::File(id)) = args.first() else {
                return Err(BuiltinError::TypeMismatch("close_file expects a file handle".into()));
            };
            close_file(*id)
        }
        "__file_exists" => {
            let Some(Value::Str(path)) = args.first() else {
                return Err(BuiltinError::TypeMismatch("file_exists expects a String path".into()));
            };
            Ok(crate::value::bool_value(std::path::Path::new(path).exists()))
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
        apply_io_builtin("__write_file", &[Value::Str(p.into()), Value::Str("data".into())])
            .unwrap();
        let r = apply_io_builtin("__read_file", &[Value::Str(p.into())]).unwrap();
        assert_eq!(r, Value::Str("data".into()));
    }

    #[test]
    fn file_exists() {
        let path = std::env::temp_dir().join("slc_io_exists_test.txt");
        let p = path.to_str().unwrap();
        std::fs::write(&path, "x").unwrap();
        let r = apply_io_builtin("__file_exists", &[Value::Str(p.into())]).unwrap();
        assert_eq!(r, crate::value::bool_value(true));
    }

    #[test]
    fn read_missing_file_fails() {
        let r = apply_io_builtin("__read_file", &[Value::Str("/nonexistent/nope".into())]);
        // Reading a missing file is an outcome the caller handles, not a
        // fault in the program.
        assert!(matches!(r, Err(BuiltinError::Failed(_))), "{r:?}");
    }
}
