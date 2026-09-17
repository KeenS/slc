// The four logical units, each the nullary form of its connective: a paren
// holding only its separator — `(,)`, `(|)`, `(&)` and `(;)`.

fn unit_value() -> (,) {
    (,)
}

// `(|)` has no value, so a function from it never has to produce one...
fn use_empty<+T>(empty: (|)) -> T {
    match empty {}
}

// ...and its consumer has no arms.
fn absurd(out: i64) <- (|) {
    select (|) {}
}

fn top_value() -> (&) {
    (&)
}

// `(;)` is what a command is.
command main | (exit: i32) -> (;) / {IO} {
    <unit_value() | println;
    top_value();
    <0 | exit>
}
