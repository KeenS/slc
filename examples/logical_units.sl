// The four logical units, and the spellings of their connectives.
//
// Each connective is written by name (`data`, `enum`, `menu`, `form`),
// anonymously — `(T1, T2)`, `(T1 | T2)`, `(T1 & T2)`, `(T1 ⅋ T2)` — and
// nullary, as a paren holding only its separator: `(,)`, `(|)`, `(&)`, `(⅋)`.
// The units have their logical names too: `1`, `0`, `⊤`, `⊥`.

fn unit_value() -> 1 {
    (,)
}

// Bottom's nullary demand is the unit value itself: dual(⊥) = 1.
fn bottom_demand() -> Unit {
    Bottom {}
}

// `0` has no value, so a function from it never has to produce one...
fn use_empty<T>(empty: (|)) -> T {
    match empty {}
}

// ...and its consumer has no arms.
fn absurd(out: i64) <- 0 {
    select (|) {}
}

fn top_value() -> ⊤ {
    (&)
}

command main | (exit: i32) -> (⅋) / {IO} {
    unit_value() | println;
    bottom_demand() | println;
    top_value();
    0 | exit⟩
}
