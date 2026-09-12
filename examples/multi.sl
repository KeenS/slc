// Several traits and several effects in one function.
//
// A function may be bound by more than one trait — one bound per type
// parameter in v1 — and may perform more than one effect, listing them all
// in its row. Traits are discharged at the call by the argument types;
// effects are discharged by handlers, one per effect, nested.

// ── Several traits ──────────────────────────────────────────────────────

trait Show  { fn show(self: +Self) -> String; }
trait Width { fn width(self: +Self) -> i64; }

impl Show for i64  { fn show(self: +i64) -> String { int_to_str(self) } }
impl Show for bool { fn show(self: +bool) -> String { if self { "yes" } else { "no" } } }
impl Show for String { fn show(self: +String) -> String { self } }
impl Width for i64 { fn width(self: +i64) -> i64 { str_len(int_to_str(self)) } }

// Two type parameters, two bounds: `show` resolves on each argument's type.
fn pair<A: Show, B: Show>(a: +A, b: +B) -> String {
    show(a) + ", " + show(b)
}

// Bounds from two different traits at once.
fn show_with_width<T: Show, N: Width>(label: +T, n: +N) -> String {
    show(label) + " (" + int_to_str(width(n)) + " digits)"
}

// ── Several effects ─────────────────────────────────────────────────────

effect Exn    { fn fail(message: +String) -> i64; }
effect Reader { fn config() -> i64; }

// The row lists every effect the body may perform.
fn scale(x: +i64) -> i64 / {Exn, Reader} {
    if x == 0 {
        fail("cannot scale zero")
    } else {
        x * config()
    }
}

command main | (exit: -i32) {
    // multiple traits, resolved per argument type
    println(pair(42, true));
    println(show_with_width("n", 1234));

    // multiple effects, discharged by nested handlers — the inner handles
    // Exn, the outer Reader. Either order works; each handler answers its
    // own operations.
    let ok = handle (handle scale(5) {
        fail(m) => 0 - 1,
        return(n) => n,
    }) {
        config(): resume => resume(10),
        return(n) => n,
    };
    println(ok);                        // 5 * 10 = 50

    let bad = handle (handle scale(0) {
        fail(m) => 0 - 1,
        return(n) => n,
    }) {
        config(): resume => resume(10),
        return(n) => n,
    };
    println(bad);                       // failed → -1

    0 @ exit
}
