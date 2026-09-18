// `num`: what the integer builtins leave to the library. These helpers use
// i64, matching the integer representation and the prelude's integer APIs.

pub fn min(a: i64, b: i64) -> i64 {
    match (<(a, b) | lt) { True => { a }, _ => { b } }
}

pub fn max(a: i64, b: i64) -> i64 {
    match (<(a, b) | gt) { True => { a }, _ => { b } }
}

pub fn abs(n: i64) -> i64 {
    match (<(n, 0) | lt) { True => { (<(0, n) | sub) }, _ => { n } }
}

pub fn signum(n: i64) -> i64 {
    match (<(n, 0) | lt) {
        True => -1,
        _ => match (<(n, 0) | gt) {
            True => 1,
            _ => 0,
        },
    }
}

pub fn is_even(n: i64) -> Bool {
    match (<(n, 2) | rem) {
        0 => True,
        _ => False,
    }
}

pub fn is_odd(n: i64) -> Bool {
    match (<(n, 2) | rem) {
        0 => False,
        _ => True,
    }
}

fn gcd_nonnegative(a: i64, b: i64) -> i64 {
    match (<(b, 0) | eq) {
        True => a,
        _ => <(b, <(a, b) | rem) | gcd_nonnegative,
    }
}

pub fn gcd(a: i64, b: i64) -> i64 {
    <(<a | abs, <b | abs) | gcd_nonnegative
}

// The result is nonnegative; zero is the least common multiple of zero and
// any integer. Dividing before multiplying leaves more headroom for values
// whose product would overflow even though the mathematical result fits.
pub fn lcm(a: i64, b: i64) -> i64 {
    match (<(a, 0) | eq) {
        True => 0,
        _ => match (<(b, 0) | eq) {
            True => 0,
            _ => {
                let common = <(a, b) | gcd;
                let reduced = <(a, common) | div;
                <(reduced, b) | mul | abs
            },
        },
    }
}

pub fn div_rem(a: i64, b: i64) -> (i64, i64) {
    // `/` and `%` retain the runtime's truncation-toward-zero semantics.
    (<(a, b) | div, <(a, b) | rem)
}
