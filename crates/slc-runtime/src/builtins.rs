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

/// `2^63`. `i64::MAX` rounds to this float, and the float is not itself an `i64`.
const TWO_63: f64 = 9223372036854775808.0;

fn float_text(value: f64) -> String {
    format!("{value}")
}

fn unfit(shown: impl std::fmt::Display, width: &str) -> BuiltinError {
    BuiltinError::ArithmeticOverflow(format!("{shown} does not fit in {width}"))
}

/// An integer as an exact `f64`. `i64::MIN` is the power of two `-2^63`.
fn exact_f64(n: i64) -> Option<f64> {
    if n == i64::MIN {
        return Some(n as f64);
    }
    let value = n as f64;
    if value.abs() < TWO_63 && value as i64 == n { Some(value) } else { None }
}

/// An integer as an exact `f32`, stored in the `f64` word both widths share.
fn exact_f32(n: i64) -> Option<f64> {
    if n == i64::MIN {
        return Some(n as f32 as f64);
    }
    let value = n as f32 as f64;
    if value.abs() < TWO_63 && value as i64 == n { Some(value) } else { None }
}

/// `x` when it is an exact `f32`. Every `NaN` fits; a rounded value does not.
fn exact_f32_from_f64(value: f64) -> Option<f64> {
    if value.is_nan() {
        return Some(value as f32 as f64);
    }
    let narrowed = value as f32 as f64;
    if narrowed.to_bits() == value.to_bits() { Some(narrowed) } else { None }
}

/// A finite integral float inside `lo..=hi`. `+2^63` is not an `i64`.
fn exact_int(value: f64, lo: i64, hi: i64) -> Option<i64> {
    if !value.is_finite() || value.trunc() != value || value >= TWO_63 || value < -TWO_63 {
        return None;
    }
    let n = value as i64;
    if n < lo || n > hi { None } else { Some(n) }
}

/// Beneath `Into`. An integer stays an integer when it is inside the
/// destination. A float stays a float when the destination is exact, and
/// becomes an integer only when it already is one. Nothing rounds.
fn convert_width(name: &str, value: &Value) -> Result<Value, BuiltinError> {
    match (name, value) {
        ("__to_f64", Value::Int(n)) => {
            exact_f64(*n).map(Value::Float).ok_or_else(|| unfit(*n, "f64"))
        }
        ("__to_f32", Value::Int(n)) => {
            exact_f32(*n).map(Value::Float).ok_or_else(|| unfit(*n, "f32"))
        }
        ("__to_f64", Value::Float(n)) => Ok(Value::Float(*n)),
        ("__to_f32", Value::Float(n)) => {
            exact_f32_from_f64(*n).map(Value::Float).ok_or_else(|| unfit(float_text(*n), "f32"))
        }
        (
            "__to_i8" | "__to_i32" | "__to_i64" | "__to_u8" | "__to_u32" | "__to_u64",
            Value::Int(n),
        ) => {
            let (width, lo, hi) =
                integer_destination(name).expect("a width builtin names its range");
            if *n < lo || *n > hi {
                return Err(unfit(*n, width));
            }
            Ok(Value::Int(*n))
        }
        (
            "__to_i8" | "__to_i32" | "__to_i64" | "__to_u8" | "__to_u32" | "__to_u64",
            Value::Float(n),
        ) => {
            let (width, lo, hi) =
                integer_destination(name).expect("a width builtin names its range");
            exact_int(*n, lo, hi).map(Value::Int).ok_or_else(|| unfit(float_text(*n), width))
        }
        _ => Err(BuiltinError::TypeMismatch(format!("{name} expects an integer or a float"))),
    }
}

thread_local! {
    static PROGRAM_ARGS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The words `slc run` passed after the file. The evaluating thread sets them.
pub fn set_program_arguments(args: Vec<String>) {
    PROGRAM_ARGS.with(|slot| *slot.borrow_mut() = args);
}

fn program_arguments() -> Vec<String> {
    PROGRAM_ARGS.with(|slot| slot.borrow().clone())
}

/// Nanoseconds since the first reading in this process. The origin is arbitrary;
/// the difference of two readings is the time the program spent between them.
fn monotonic_ns() -> Result<i64, BuiltinError> {
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let origin = ORIGIN.get_or_init(std::time::Instant::now);
    let nanos = std::time::Instant::now().saturating_duration_since(*origin).as_nanos();
    i64::try_from(nanos).map_err(|_| {
        BuiltinError::ArithmeticOverflow("the monotonic clock does not fit in i64".into())
    })
}

fn real_unary(name: &str, value: f64) -> Result<f64, BuiltinError> {
    match name {
        "__sqrt" if value < 0.0 => {
            Err(BuiltinError::ArithmeticOverflow(format!("sqrt({})", float_text(value))))
        }
        "__sqrt" => Ok(value.sqrt()),
        "__abs" => Ok(value.abs()),
        "__floor" => Ok(value.floor()),
        "__ceil" => Ok(value.ceil()),
        _ => Err(BuiltinError::UnknownBuiltin(name.to_string())),
    }
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
        // Beneath `Into`. Integers and floats meet in both directions, and a
        // value that is not exact for the destination does not fit.
        "__to_i8" | "__to_i32" | "__to_i64" | "__to_u8" | "__to_u32" | "__to_u64" | "__to_f32"
        | "__to_f64" => match args.first() {
            Some(value) => convert_width(name, value),
            None => {
                Err(BuiltinError::TypeMismatch(format!("{name} expects an integer or a float")))
            }
        },
        "__sqrt" | "__abs" | "__floor" | "__ceil" => match args.first() {
            Some(Value::Float(n)) => real_unary(name, *n).map(Value::Float),
            _ => Err(BuiltinError::TypeMismatch(format!("{name} expects a float argument"))),
        },
        "__argument_count" => Ok(Value::Int(program_arguments().len() as i64)),
        "__argument_at" => match args.first() {
            Some(Value::Int(index)) => program_arguments()
                .get(usize::try_from(*index).unwrap_or(usize::MAX))
                .cloned()
                .map(Value::Str)
                .ok_or_else(|| BuiltinError::TypeMismatch(format!("index {index} out of range"))),
            _ => Err(BuiltinError::TypeMismatch("__argument_at expects an i64".into())),
        },
        "__monotonic_ns" => monotonic_ns().map(Value::Int),
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
            Err(BuiltinError::ArithmeticOverflow(ref m)) if *m == format!("neg({})", i64::MIN)
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
        assert!(matches!(narrow, Err(BuiltinError::ArithmeticOverflow(ref m)) if m.contains("i8")));
        let negative = apply_builtin("__to_u64", &[Value::Int(-1)], &mut buf);
        assert!(matches!(negative, Err(BuiltinError::ArithmeticOverflow(ref m)) if m.contains("u64")));
    }

    #[test]
    fn integers_and_floats_meet_only_when_the_value_is_exact() {
        let mut buf = Vec::new();
        let widened = apply_builtin("__to_f64", &[Value::Int(4)], &mut buf).unwrap();
        assert_eq!(widened, Value::Float(4.0));
        let min = apply_builtin("__to_f64", &[Value::Int(i64::MIN)], &mut buf).unwrap();
        assert_eq!(min, Value::Float(i64::MIN as f64));
        let power = apply_builtin("__to_f32", &[Value::Int(1 << 24)], &mut buf).unwrap();
        assert_eq!(power, Value::Float((1i64 << 24) as f64));
        let inexact = apply_builtin("__to_f64", &[Value::Int(i64::MAX)], &mut buf);
        assert!(
            matches!(inexact, Err(BuiltinError::ArithmeticOverflow(ref m)) if m.contains("does not fit in f64")),
            "{inexact:?}"
        );
        let past_f32 = apply_builtin("__to_f32", &[Value::Int((1 << 24) + 1)], &mut buf);
        assert!(
            matches!(past_f32, Err(BuiltinError::ArithmeticOverflow(ref m)) if m.contains("f32")),
            "{past_f32:?}"
        );
        let whole = apply_builtin("__to_i64", &[Value::Float(4.0)], &mut buf).unwrap();
        assert_eq!(whole, Value::Int(4));
        let signed_zero = apply_builtin("__to_u8", &[Value::Float(-0.0)], &mut buf).unwrap();
        assert_eq!(signed_zero, Value::Int(0));
        let fraction = apply_builtin("__to_i64", &[Value::Float(1.5)], &mut buf);
        assert!(
            matches!(fraction, Err(BuiltinError::ArithmeticOverflow(ref m)) if m == "1.5 does not fit in i64"),
            "{fraction:?}"
        );
        let nan = apply_builtin("__to_i32", &[Value::Float(f64::NAN)], &mut buf);
        assert!(
            matches!(nan, Err(BuiltinError::ArithmeticOverflow(ref m)) if m.contains("NaN")),
            "{nan:?}"
        );
        let narrowed = apply_builtin("__to_f32", &[Value::Float(1.25)], &mut buf).unwrap();
        assert_eq!(narrowed, Value::Float(1.25));
        let rounded = apply_builtin("__to_f32", &[Value::Float(16_777_217.0)], &mut buf);
        assert!(
            matches!(rounded, Err(BuiltinError::ArithmeticOverflow(ref m)) if m.contains("f32")),
            "{rounded:?}"
        );
        let kept_nan = apply_builtin("__to_f32", &[Value::Float(f64::NAN)], &mut buf).unwrap();
        assert!(matches!(kept_nan, Value::Float(n) if n.is_nan()));
        let neg_zero = apply_builtin("__to_f32", &[Value::Float(-0.0)], &mut buf).unwrap();
        assert!(matches!(neg_zero, Value::Float(n) if n.to_bits() == (-0.0f64).to_bits()));
    }

    #[test]
    fn square_root_rejects_a_negative_and_the_other_reals_are_total() {
        let mut buf = Vec::new();
        let root = apply_builtin("__sqrt", &[Value::Float(4.0)], &mut buf).unwrap();
        assert_eq!(root, Value::Float(2.0));
        let negative = apply_builtin("__sqrt", &[Value::Float(-1.0)], &mut buf);
        assert!(
            matches!(negative, Err(BuiltinError::ArithmeticOverflow(ref m)) if m == "sqrt(-1)"),
            "{negative:?}"
        );
        let nan = apply_builtin("__sqrt", &[Value::Float(f64::NAN)], &mut buf).unwrap();
        assert!(matches!(nan, Value::Float(n) if n.is_nan()));
        let abs = apply_builtin("__abs", &[Value::Float(-3.25)], &mut buf).unwrap();
        assert_eq!(abs, Value::Float(3.25));
        let floor = apply_builtin("__floor", &[Value::Float(-1.5)], &mut buf).unwrap();
        assert_eq!(floor, Value::Float(-2.0));
        let ceil = apply_builtin("__ceil", &[Value::Float(1.2)], &mut buf).unwrap();
        assert_eq!(ceil, Value::Float(2.0));
    }

    #[test]
    fn program_arguments_are_the_words_set_for_this_thread() {
        let mut buf = Vec::new();
        set_program_arguments(vec!["one".into(), "two".into()]);
        let count = apply_builtin("__argument_count", &[Value::Unit], &mut buf).unwrap();
        assert_eq!(count, Value::Int(2));
        let first = apply_builtin("__argument_at", &[Value::Int(0)], &mut buf).unwrap();
        assert_eq!(first, Value::Str("one".into()));
        let missing = apply_builtin("__argument_at", &[Value::Int(2)], &mut buf);
        assert!(matches!(missing, Err(BuiltinError::TypeMismatch(_))), "{missing:?}");
        set_program_arguments(Vec::new());
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
