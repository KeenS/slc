// Functions over data types, in both polarities.
//
// A `data` is a positive product and an `enum` a positive sum: data you
// build and take apart. A positive function maps data to data. A negative
// function is its mirror image: what follows `<-` is the type it consumes,
// and its parameters are the continuations the answer leaves through — so
// "taking" and "returning" swap sides of the arrow.
//
//                 takes data                      returns data
//   positive      fn f(p: Point) -> …             fn f(…) -> Point
//   negative      fn f(…) <- Point                fn f(out: -Point) <- …

data Point {
    x: i64,
    y: i64,
}

enum Size {
    Small,
    Big(i64), // how far over the limit
}

// ─── Positive ────────────────────────────────────────────────────────────

// Takes a record and returns one: build a new value from the old.
func translate(p: Point, dx: i64, dy: i64) -> Point {
    Point { x: (<(p.x, dx) | add), y: (<(p.y, dy) | add) }
}

// Takes a record and returns an enum: the result is one tagged variant.
func classify(p: Point) -> Size {
    let area = <(p.x, p.y) | mul;
    of (<(area, 100) | gt) {
        True => Size::Big(<(area, 100) | sub),
        False => Size::Small,
    }
}

// Takes an enum: `of` chooses the branch the variant selects.
func overshoot(s: Size) -> i64 {
    of s {
        Small => 0,
        Big(over) => over,
    }
}

// ─── Negative ────────────────────────────────────────────────────────────

// "Takes" a record: the function is a consumer of `Point`, and the arm
// binds every field at once.
func area_of(out: i64) <- Point {
    mu Point {
        Point { x, y } => <(x, y) | mul | out>,
    }
}

// Takes a record and "returns" one: the new value is cut against `out`
// instead of travelling back through a return.
func reflect(out: Point) <- Point {
    mu Point {
        Point { x, y } => <Point { x: y, y: x } | out>,
    }
}

// "Returns" an enum: consume a bare number, send one variant onward.
func classify_to(out: Size) <- i64 {
    mu i64 {
        area => of (<(area, 100) | gt) {
            True => <Size::Big(<(area, 100) | sub) | out>,
            False => <Size::Small | out>,
        },
    }
}

// "Takes" an enum: one arm per variant; only the arriving variant runs.
func overshoot_of(out: i64) <- Size {
    mu Size {
        Small => <0 | out>,
        Big(over) => <over | out>,
    }
}

proc main | (exit: i32) / {IO} {
    // Positive: data flows inward through the calls and back out.
    let p = <(Point { x: 3, y: 4 }, 7, 16) | translate; // Point { x: 10, y: 20 }
    <p.x | println; // 10
    <p | classify | overshoot | println; // 100 = 10 * 20 - 100

    // Negative: the same computations, written in the order the value
    // travels. A consumer written with `<-` flows like the function it
    // equally is.
    <Point { x: 10, y: 20 } | area_of | println; // 200

    let r = <Point { x: 1, y: 2 } | reflect;
    <r.x | println; // 2

    // `classify_to` sends a `Size` on and `overshoot_of` consumes one: the
    // enum passes between them without a name.
    <150 | classify_to | overshoot_of | println; // 50

    <0 | exit>
}
