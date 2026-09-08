// This program is intentionally ill-typed: a `fn` parameter must have
// positive polarity, but `-i32` is negative.
//
// Run it to see the diagnostic:
//
//   slc run examples/polarity_error.sl

fn bad(x: -i32) -> i32 {
    x
}

fn main() -> i32 {
    42
}
