// This program is intentionally ill-typed: a `mu` value parameter must
// have positive polarity, but `-i32` is negative.
//
// Run it to see the diagnostic:
//
//   slc run examples/polarity_error.sl

mu bad(x: -i32, to k: -i32) {
    k(x)
}

fn main() -> i32 {
    42
}
