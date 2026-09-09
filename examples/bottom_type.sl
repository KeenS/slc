// Bottom type.
//
// `⊥` is the dual of unit: it is inhabited on the negative side only.
// A continuation of type `-⊥` accepts an impossible value, so once a
// continuation is supplied, the rest of the computation need not return
// normally.

fn absurd(k: -⊥) -> i64 {
    match k {
        _ => 42,
    }
}

fn main() -> i64 {
    absurd(0)
}
