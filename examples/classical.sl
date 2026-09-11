// Classical control: double negation elimination and excluded middle.
//
// The core is the classical λ̄μμ̃ calculus, so both laws are ordinary
// programs. What makes them work is `mu`, which captures the continuation of
// the expression it stands in and hands it to someone else.
//
// Negation is a consumer: `¬A` is `-A`, because `A → ⊥` and `-A` are one
// type. A consumer is not data, though — it travels only in a box, `↓-A` —
// so negating again goes through the shift: a refuter consumes `↓-i64`, its
// own type is `dual(↓-i64) = ↑i64`, and boxed as an argument it is `↓↑i64`.
// The shifts never cancel, so `¬¬i64` is not `i64`: double negation
// elimination is a program, not a definition.

// Capture this call's continuation, box it, and send it to the refuter.
// Whatever the refuter puts in the box is what the call returns.
fn dne(refuter: ↓↑i64) -> i64 {
    mu { k <= ↓k @ ↑refuter }
}

// A refuter: hand it a boxed consumer of `i64`, and it fills it with 42.
fn any_refuter() -> ↓↑i64 {
    ↓select ↓-i64 {
        boxed <= 42 @ ↑boxed,
    }
}

// `A ⊕ ¬A`, as data: either the value, or something that refutes it.
enum Choice {
    Holds(i64),
    // A consumer is not data until it is boxed: `↓-i64`. The box is what
    // keeps `¬¬A` from collapsing to `A`.
    Refutes(↓-i64),
}

// The classical move: answer `Refutes`, holding a consumer that — if anyone
// ever supplies a value to it, however much later — makes this same
// expression have answered `Holds` instead. The refutation is this call's
// own continuation, dressed as a consumer of `i64`.
fn lem() -> Choice {
    mu { k <= Choice::Refutes(↓select +i64 { a <= Choice::Holds(a) @ k }) @ k }
}

command main | (exit: -i32) {
    // ¬¬A → A
    println(dne(any_refuter()));

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
            println("holds: " + int_to_str(n));
            0 @ exit
        },
        Refutes(r) => {
            println("refuted — taking the offer");
            42 @ ↑r
        },
    }
}
