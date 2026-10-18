//! Exact width conversions and real-function domains.
//!
//! The interpreter and the native runtime both call these, so a value fits
//! in one engine exactly when it fits in the other.

/// `2^63`. `i64::MAX` rounds to this float, and the float is not itself an `i64`.
const TWO_63: f64 = 9223372036854775808.0;

/// A float printed the way an overflow diagnostic shows it.
pub fn float_text(value: f64) -> String {
    format!("{value}")
}

/// Inclusive bounds of an integer width. `u64` stops at `i64::MAX`: the
/// machine word is signed.
pub fn int_bounds(width: &str) -> Option<(&'static str, i64, i64)> {
    Some(match width {
        "i8" => ("i8", i64::from(i8::MIN), i64::from(i8::MAX)),
        "i32" => ("i32", i64::from(i32::MIN), i64::from(i32::MAX)),
        "i64" => ("i64", i64::MIN, i64::MAX),
        "u8" => ("u8", 0, i64::from(u8::MAX)),
        "u32" => ("u32", 0, i64::from(u32::MAX)),
        "u64" => ("u64", 0, i64::MAX),
        _ => return None,
    })
}

/// An integer as an exact `f64`. `i64::MIN` is the power of two `-2^63`.
pub fn exact_f64(n: i64) -> Option<f64> {
    if n == i64::MIN {
        return Some(n as f64);
    }
    let value = n as f64;
    if value.abs() < TWO_63 && value as i64 == n { Some(value) } else { None }
}

/// An integer as an exact `f32`, stored in the `f64` word both widths share.
pub fn exact_f32(n: i64) -> Option<f64> {
    if n == i64::MIN {
        return Some(n as f32 as f64);
    }
    let value = n as f32 as f64;
    if value.abs() < TWO_63 && value as i64 == n { Some(value) } else { None }
}

/// `x` when it is an exact `f32`. Every `NaN` fits; a rounded value does not.
pub fn exact_f32_from_f64(value: f64) -> Option<f64> {
    if value.is_nan() {
        return Some(value as f32 as f64);
    }
    let narrowed = value as f32 as f64;
    if narrowed.to_bits() == value.to_bits() { Some(narrowed) } else { None }
}

/// A finite integral float inside `lo..=hi`. `+2^63` is not an `i64`.
pub fn exact_int(value: f64, lo: i64, hi: i64) -> Option<i64> {
    if !value.is_finite() || value.trunc() != value || value >= TWO_63 || value < -TWO_63 {
        return None;
    }
    let n = value as i64;
    if n < lo || n > hi { None } else { Some(n) }
}

/// The shift distance is the count's low six bits.
pub fn shift_count(count: i64) -> u32 {
    (count as u32) & 63
}

/// `Err` is `sqrt(x)` when `x` is negative. `NaN` is in the domain.
pub fn real_sqrt(value: f64) -> Result<f64, String> {
    if value < 0.0 { Err(format!("sqrt({})", float_text(value))) } else { Ok(value.sqrt()) }
}

/// `Err` is `ln(x)` when `x` is not positive. `NaN` is in the domain.
pub fn real_ln(value: f64) -> Result<f64, String> {
    if value <= 0.0 { Err(format!("ln({})", float_text(value))) } else { Ok(value.ln()) }
}

/// IEEE power. A negative base with a non-integral exponent, and a zero base
/// with a negative exponent, are outside the domain. `Err` is `pow(base, exp)`.
pub fn real_pow(base: f64, exp: f64) -> Result<f64, String> {
    let outside =
        (base == 0.0 && exp < 0.0) || (base < 0.0 && exp.is_finite() && exp.fract() != 0.0);
    if !base.is_nan() && !exp.is_nan() && outside {
        Err(format!("pow({}, {})", float_text(base), float_text(exp)))
    } else {
        Ok(base.powf(exp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powers_of_two_fit_and_rounded_integers_do_not() {
        assert_eq!(exact_f64(i64::MIN), Some(i64::MIN as f64));
        assert_eq!(exact_f32(i64::MIN).map(f64::to_bits), Some((i64::MIN as f32 as f64).to_bits()));
        assert_eq!(exact_f64(i64::MAX), None);
        assert_eq!(exact_int(1.5, i64::MIN, i64::MAX), None);
        assert_eq!(float_text(1.5), "1.5");
        let (_, lo, hi) = int_bounds("i8").unwrap();
        assert_eq!(exact_int(200.0, lo, hi), None);
        assert_eq!(int_bounds("u64").unwrap().2, i64::MAX);
        assert_eq!(int_bounds("f64"), None);
    }

    #[test]
    fn real_domains_name_the_refused_value() {
        assert_eq!(real_sqrt(-1.0), Err("sqrt(-1)".into()));
        assert!(real_sqrt(f64::NAN).unwrap().is_nan());
        assert_eq!(real_ln(0.0), Err("ln(0)".into()));
        assert_eq!(real_pow(-1.5, 0.5), Err("pow(-1.5, 0.5)".into()));
        assert_eq!(real_pow(0.0, -1.0), Err("pow(0, -1)".into()));
        assert_eq!(real_pow(2.0, 3.0).unwrap(), 8.0);
    }

    #[test]
    fn shift_counts_keep_six_bits() {
        assert_eq!(shift_count(64), 0);
        assert_eq!(shift_count(-1), 63);
        assert_eq!(shift_count(3), 3);
    }
}
