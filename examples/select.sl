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
        Red => 0 @ return,
        Green => 1 @ return,
        Blue => 2 @ return,
    }
}

// A variant may carry a payload, and an arm binds it for its consumer.
enum Reading {
    Measured(i64),
    Missing,
}

fn report(value: -i64 & absent: -i64) <- Reading {
    select Reading {
        Measured(measurement) => measurement @ value,
        Missing => -1 @ absent,
    }
}

// A positive type with one shape and one component — an atom — needs only one
// arm, whose pattern is a plain binder naming the whole value. That consumer
// is the core's value abstraction `μ̃x. c`: the same binder `let` lowers to,
// written directly.
fn twice(out: -i64) <- +i64 {
    select +i64 {
        n => (n * 2) @ out,
    }
}

command main | (exit: -i32) {
    println(mu i32 { answer <= Color::Green @ code(answer) });
    println(mu i64 { answer <= Reading::Measured(42) @ report(answer, answer) });
    println(mu i64 { answer <= 50 @ twice(answer) });
    0 @ exit
}
