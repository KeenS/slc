// `num`: what the integer builtins leave to the library.

mod num {
    pub fn min(a: i64, b: i64) -> i64 {
        match (⟨(a, b) | lt) { True => { a }, _ => { b } }
    }

    pub fn max(a: i64, b: i64) -> i64 {
        match (⟨(a, b) | gt) { True => { a }, _ => { b } }
    }

    pub fn abs(n: i64) -> i64 {
        match (⟨(n, 0) | lt) { True => { (⟨(0, n) | sub) }, _ => { n } }
    }
}
