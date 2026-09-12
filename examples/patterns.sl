// A binder is a pattern.
//
// `let p = e`, and a parameter `p: T`, take a pattern rather than a bare
// name — the name being the trivial pattern. The rule is Rust's: a binder
// stands for *every* value of its type, since there is no other arm to fall
// to, so the pattern must be irrefutable. Anything that can fail to match
// belongs in a `match`, which says what happens when it does.
//
// This is also what the unary calling convention already stood on. A
// declaration binds one argument per group, and a header *is* a pattern with
// typed leaves: the value group a tuple pattern on the one argument, the
// continuation group a bundle pattern on the one menu of exits. Writing a
// pattern at a leaf only says out loud what the group was doing already.

data Point { x: i64, y: i64 }

// One variant, so naming it covers the type: a sum of one has one shape.
enum Wrapped {
    Only(i64),
}

// The value group is a product of two, and its first leaf is itself a
// product. `fn f((a, b): (…), c: …)` needs nothing new — the argument was
// always one packed value, and this destructures it a level deeper.
fn skew((a, b): (i64 ⊗ i64), c: i64) -> i64 {
    a * c - b
}

// A record leaf, taken apart in the header rather than the body.
fn norm(Point { x, y }: Point) -> i64 {
    x * x + y * y
}

// A `command` does the same, and its exits stay names: control leaves
// through a name, and a pattern has nowhere to leave through.
command nearer((here, there): (Point ⊗ Point)) | (closer: Point) {
    if (here | norm) < (there | norm) { here | closer⟩ } else { there | closer⟩ }
}

command main | (exit: i32) / {IO} {
    // A tuple binder, and a nested one.
    let (a, b) = (3, 4);
    let ((p, q), r) = ((1, 2), 3);
    a + b + p + q + r | println;                       // 13

    // A record binder, and a single-variant enum.
    let Point { x, y } = Point { x: 6, y: 7 };
    let Only(n) = Only(29);
    x * y + n | println;                               // 71

    // `_` binds nothing, as it does in a `match` arm.
    let _ = "evaluated, then dropped";

    ((3, 4), 5) | skew | println;                      // 11
    Point { x: 3, y: 4 } | norm | println;             // 25

    let winner = mu Point {
        closer <= (Point { x: 1, y: 1 }, Point { x: 9, y: 9 }) | nearer | closer⟩
    };
    winner.x + winner.y | println;                     // 2

    0 | exit⟩
}
