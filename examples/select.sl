// Negative additive construction.
//
// `enum` constructs a positive additive sum: a value that is one of several
// variants. `match` eliminates it by choosing a branch.
//
// `select` constructs the dual: a negative additive continuation. It consumes
// an existing enum and returns a continuation. Activating that continuation
// with an enum value dispatches to the matching arm, binds the variant's
// payload, and activates that arm's consumer. Exactly one arm runs; the
// others are never evaluated.

enum Color {
    Red,
    Green,
    Blue,
}

// A negative function: it consumes the continuation `return` and produces the
// continuation that a `Color` is cut against.
fn code(return: -i32) <- Color {
    select Color {
        0 @ return => Red,
        1 @ return => Green,
        2 @ return => Blue,
    }
}

// A variant may carry a payload, and an arm binds it for its consumer.
enum Reading {
    Measured(i64),
    Missing,
}

fn report(value: -i64, absent: -i64) <- Reading {
    select Reading {
        measurement @ value => Measured(measurement),
        -1 @ absent => Missing,
    }
}

mu main() | (exit: -i32) {
    println(mu ask() | (answer: -i32) {
        Color::Green @ code(answer)
    });
    println(mu ask() | (answer: -i64) {
        Reading::Measured(42) @ report(answer, answer)
    });
    0 @ exit
}
