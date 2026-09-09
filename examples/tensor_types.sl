// Tensor types.
//
// `A ⊗ B` is the positive multiplicative conjunction: a value carrying
// both an A and a B. Surface tuples lower to nested tensor pairs, and
// match destructuring recovers both components.

fn sum_pair(p: (+i64 ⊗ +i64)) -> i64 {
    match p {
        (a, b) => add(a, b),
        _ => 0,
    }
}

fn main() -> i64 {
    sum_pair((10, 20))
}
