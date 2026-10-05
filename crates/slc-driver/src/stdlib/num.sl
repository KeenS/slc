// `num`: integer helpers on i64, and `min`/`max` over anything `Ord` orders.
// Equal arguments answer the second. `abs` stays on i64; a float absolute
// value is the prelude's `abs`.

pub func min<+T: Ord>(a: T, b: T) -> T {
    of (<(a, b) | lt) { True => { a }, _ => { b } }
}

pub func max<+T: Ord>(a: T, b: T) -> T {
    of (<(a, b) | gt) { True => { a }, _ => { b } }
}

pub func abs(n: i64) -> i64 {
    of (<(n, 0) | lt) { True => { (<(0, n) | sub) }, _ => { n } }
}

pub func signum(n: i64) -> i64 {
    of (<(n, 0) | lt) {
        True => -1,
        _ => of (<(n, 0) | gt) {
            True => 1,
            _ => 0,
        },
    }
}

pub func is_even(n: i64) -> Bool {
    of (<(n, 2) | rem) {
        0 => True,
        _ => False,
    }
}

pub func is_odd(n: i64) -> Bool {
    of (<(n, 2) | rem) {
        0 => False,
        _ => True,
    }
}

func gcd_nonnegative(a: i64, b: i64) -> i64 {
    of (<(b, 0) | eq) {
        True => a,
        _ => <(b, <(a, b) | rem) | gcd_nonnegative,
    }
}

pub func gcd(a: i64, b: i64) -> i64 {
    <(<a | abs, <b | abs) | gcd_nonnegative
}

// The result is nonnegative; zero is the least common multiple of zero and
// any integer. Dividing before multiplying leaves more headroom for values
// whose product would overflow even though the mathematical result fits.
pub func lcm(a: i64, b: i64) -> i64 {
    of (<(a, 0) | eq) {
        True => 0,
        _ => of (<(b, 0) | eq) {
            True => 0,
            _ => {
                let common = <(a, b) | gcd;
                let reduced = <(a, common) | div;
                <(reduced, b) | mul | abs
            },
        },
    }
}

pub func div_rem(a: i64, b: i64) -> (i64, i64) {
    // `/` and `%` retain the runtime's truncation-toward-zero semantics.
    (<(a, b) | div, <(a, b) | rem)
}
