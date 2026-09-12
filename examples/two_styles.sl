// The same program, twice: value-first, then continuation-first.
//
// The calculus is symmetric, so every program has a mirror image. Reading it
// in the mirror swaps each construct for its dual:
//
//   value-centric                     continuation-centric
//   -------------------------------   ------------------------------------
//   fn f(x: +A) -> B                  fn f(k: -B) <- A
//   data arrives, data leaves         a consumer arrives, a consumer leaves
//   match v { p => e }                select A { p => c }
//   return the result                 cut the result against a consumer
//   f(x)                              x | f(k)
//   g(f(x))          — inside out     x | f(g(k))        — left to right
//   let y = f(x); rest                x | f(select +B { y => rest })
//
// Both halves below compute the area of a shape, decide whether it is big or
// small, and print that. Neither is a transcription of the other: each is the
// ordinary way to write the program on its own side of the mirror.

enum Shape {
    Circle(i64),
    Rect(i64, i64),
}

// ─── Value-centric ───────────────────────────────────────────────────────
//
// A function receives data and gives data back. `match` takes the shape
// apart, and the answer travels outward through the calls.

fn area(s: Shape) -> i64 {
    match s {
        Circle(r) => 3 * r * r,
        Rect(w, h) => w * h,
    }
}

fn label(a: +i64) -> String {
    if a > 50 {
        "big"
    } else {
        "small"
    }
}

// ─── Continuation-centric ────────────────────────────────────────────────
//
// A function receives a consumer and gives a consumer back. What follows
// `<-` is the type consumed, so `area_of` is a consumer of `Shape`: give it
// somewhere to send an area, and it will accept shapes. `select` takes the
// shape apart exactly as `match` does — one arm per variant, binding the same
// payload — but each arm ends in a cut instead of producing a value.

fn area_of(out: -i64) <- Shape {
    select Shape {
        Circle(r) => ⟨(3 * r * r) | out⟩,
        Rect(w, h) => ⟨(w * h) | out⟩,
    }
}

fn label_of(out: -String) <- +i64 {
    select +i64 {
        a => if a > 50 {
            ⟨"big" | out⟩
        } else {
            ⟨"small" | out⟩
        },
    }
}

command main | (exit: -i32) {
    // Value-first: the shape goes in at the innermost call, and the answer
    // comes back out through `area` and then `label` to `println`.
    println(label(area(Shape::Circle(5))));
    println(label(area(Shape::Rect(6, 7))));

    // Continuation-first: the same pipeline, written in the order the value
    // travels. `label_of(…)` is a consumer of areas, `area_of(…)` wraps it
    // into a consumer of shapes, and the cut sets the whole thing going.
    //
    // Nothing returns here, so what comes next is written inside the last
    // consumer: the rest of the program *is* the continuation. That nesting
    // is exactly what a local `mu` removes — see `file_io.sl`.
    ⟨Shape::Circle(5) | area_of(label_of(select +String {
        first => {
            println(first);
            ⟨Shape::Rect(6, 7) | area_of(label_of(select +String {
                second => {
                    println(second);
                    ⟨0 | exit⟩
                },
            }))⟩
        },
    }))⟩
}
