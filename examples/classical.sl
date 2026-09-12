// Classical control: double negation elimination and excluded middle.
//
// The core is the classical λ̄μμ̃ calculus, and the surface wears it
// plainly: negation is a consumer — `¬A` is `-A`, because `A → ⊥` and
// `-A` are one type — and a consumer is a value like any other. So
// negation is an involution on the nose, `-(-A)` *is* `A`, and double
// negation elimination is not a program but the identity:

fn dne<T>(t: -(-T)) -> T {
    t
}

// `A ⊕ ¬A`, as data: either the value, or a consumer that refutes it —
// carried bare, since a continuation travels like any payload.
enum Choice {
    Holds(i64),
    Refutes(-i64),
}

// The classical move: answer `Refutes`, holding a consumer that — if anyone
// ever supplies a value to it, however much later — makes this same
// expression have answered `Holds` instead. The refutation is this call's
// own continuation, dressed as a consumer of `i64`.
fn lem() -> Choice {
    mu { k <= Choice::Refutes(select +i64 { a => Choice::Holds(a) | k⟩ }) | k⟩ }
}

command main | (exit: -i32) {
    // ¬¬A → A is definitional now: `-(-i64)` and `+i64` are one type.
    42 | dne | println;

    // A ⊕ ¬A. `lem()` answers `Refutes` — and taking the offer sends 42
    // back through the continuation `lem` captured, re-entering this same
    // `match` even though the call answered long ago. The second time
    // around, the same expression has produced `Holds` after all.
    //
    // A continuation is the machine's frame stack, held as a value: it can
    // be reinstated after its `mu` has answered, which is exactly what the
    // classical reading of ⊕ promises.
    match lem() {
        Holds(n) => {
            "holds: " + (n | int_to_str) | println;
            0 | exit⟩
        },
        Refutes(r) => {
            "refuted — taking the offer" | println;
            42 | r⟩
        },
    }
}
