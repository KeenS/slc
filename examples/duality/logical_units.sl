// The four logical units, each the nullary form of its connective: a paren
// holding only its separator — `(,)`, `(|)`, `(&)` and `(;)`.

func unit_value() -> (,) {
    (,)
}

// `(|)` has no value, so a function from it never has to produce one...
func use_empty<+T>(empty: (|)) -> T {
    of empty {}
}

// ...and its consumer has no arms.
func absurd(out: i64) <- (|) {
    mu (|) {}
}

func top_value() -> (&) {
    (&)
}

// `(;)` is what a command is.
proc main | (exit: i32) -> (;) / {IO} {
    <unit_value() | println;
    top_value();
    <0 | exit>
}
