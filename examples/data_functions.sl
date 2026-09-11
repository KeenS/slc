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
fn translate(p: Point, dx: +i64, dy: +i64) -> Point {
    Point { x: p.x + dx, y: p.y + dy }
}

// Takes a record and returns an enum: the result is one tagged variant.
fn classify(p: Point) -> Size {
    let area = p.x * p.y;
    if area > 100 {
        Size::Big(area - 100)
    } else {
        Size::Small
    }
}

// Takes an enum: `match` chooses the branch the variant selects.
fn overshoot(s: Size) -> i64 {
    match s {
        Small => 0,
        Big(over) => over,
    }
}

// ─── Negative ────────────────────────────────────────────────────────────

// "Takes" a record: the function is a consumer of `Point`, and the arm
// binds every field at once.
fn area_of(out: -i64) <- Point {
    select Point {
        Point { x, y } => (x * y) @ out,
    }
}

// Takes a record and "returns" one: the new value is cut against `out`
// instead of travelling back through a return.
fn reflect(out: -Point) <- Point {
    select Point {
        Point { x, y } => Point { x: y, y: x } @ out,
    }
}

// "Returns" an enum: consume a bare number, send one variant onward.
fn classify_to(out: -Size) <- +i64 {
    select +i64 {
        area => if area > 100 {
            Size::Big(area - 100) @ out
        } else {
            Size::Small @ out
        },
    }
}

// "Takes" an enum: one arm per variant; only the arriving variant runs.
fn overshoot_of(out: -i64) <- Size {
    select Size {
        Small => 0 @ out,
        Big(over) => over @ out,
    }
}

command main | (exit: -i32) {
    // Positive: data flows inward through the calls and back out.
    let p = translate(Point { x: 3, y: 4 }, 7, 16); // Point { x: 10, y: 20 }
    println(p.x);                                   // 10
    println(overshoot(classify(p)));                // 100 = 10 * 20 - 100

    // Negative: the same computations, written in the order the value
    // travels. `mu` names the hole the answer comes back through.
    println(mu i64 { answer <= {
        Point { x: 10, y: 20 } @ area_of(answer)    // 200
    } });

    let r = mu Point { answer <= Point { x: 1, y: 2 } @ reflect(answer) };
    println(r.x);                                   // 2

    // `overshoot_of(answer)` is a consumer of `Size`, exactly what
    // `classify_to` wants: the enum passes between them without a name.
    println(mu i64 { answer <= {
        150 @ classify_to(overshoot_of(answer))     // 50
    } });

    0 @ exit
}
