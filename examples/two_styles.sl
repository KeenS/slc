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
//   x | f                             x | f_of | k⟩
//   x | f | g                         x | f_of | g_of | k⟩
//   let y = x | f; rest               x | f_of(select +B { y => rest })⟩
//
// The last two rows are the point: `f: A -> B` and `f_of(k: -B) <- A` are
// *one type* — `;` is commutative, so `(-A ; B)` read the other way round is
// `(dual(B) ; dual(A))` — and a pipeline takes either. The two halves below
// therefore end up written the same way, which is what "the same program,
// twice" was always trying to say.
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

fn label(a: i64) -> String {
    match a > 50 { true => {
        "big"
    }, _ => {
        "small"
    } }
}

// ─── Continuation-centric ────────────────────────────────────────────────
//
// A function receives a consumer and gives a consumer back. What follows
// `<-` is the type consumed, so `area_of` is a consumer of `Shape`: give it
// somewhere to send an area, and it will accept shapes. `select` takes the
// shape apart exactly as `match` does — one arm per variant, binding the same
// payload — but each arm ends in a cut instead of producing a value.

fn area_of(out: i64) <- Shape {
    select Shape {
        Circle(r) => ⟨(3 * r * r) | out⟩,
        Rect(w, h) => ⟨(w * h) | out⟩,
    }
}

fn label_of(out: String) <- i64 {
    select i64 {
        a => match a > 50 { true => {
            ⟨"big" | out⟩
        }, _ => {
            ⟨"small" | out⟩
        } },
    }
}

command main | (exit: i32) / {IO} {
    // Value-first: the shape flows through `area`, then `label`.
    ⟨Shape::Circle(5) | area | label | println;
    ⟨Shape::Rect(6, 7) | area | label | println;

    // Continuation-first: the same chain, stage for stage. Each `_of` is a
    // consumer transformer, and a pipeline reads it as the function it
    // equally is — so nothing nests and nothing is written backwards.
    (⟨mu String { out <= ⟨Shape::Circle(5) | area_of | label_of | out⟩ } | println);
    (⟨mu String { out <= ⟨Shape::Rect(6, 7) | area_of | label_of | out⟩ } | println);

    // Where the two differ is what they *are*: the value-first chain
    // returns a String, and the continuation-first one ends in a cut. The
    // `mu` above is what turns the second back into a value; without it,
    // the rest of the program is written inside the last consumer.
    ⟨Shape::Circle(5) | area_of | label_of | select String {
        answer => {
            ⟨"and directly: " + answer | println;
            ⟨0 | exit⟩
        },
    }⟩
}
