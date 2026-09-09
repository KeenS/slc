// Par types.
//
// `A ⅋ B` is the negative multiplicative conjunction: the dual of
// `A ⊗ B`. Instead of producing two positive values, a par-typed
// continuation can consume both sides. This example annotates the
// continuation parameter with a par type. The continuation therefore
// represents the joint consumption of both positive sides, rather than two
// independently activated continuations.

mu consume_pair(k: -(+i64 ⅋ +i64)) {
    k(0)
}

fn main() -> i64 {
    consume_pair(fn(value: +i64) -> i64 { value })
}
