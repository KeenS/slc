// `num`: what the integer builtins leave to the library.

mod num {
    pub fn min(a: i64, b: i64) -> i64 {
        if a < b { a } else { b }
    }

    pub fn max(a: i64, b: i64) -> i64 {
        if a > b { a } else { b }
    }

    pub fn abs(n: i64) -> i64 {
        if n < 0 { 0 - n } else { n }
    }
}
