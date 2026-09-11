// The Slant prelude: ordinary declarations, available to every program.
//
// Nothing here is special to the compiler. The driver appends this file to
// the program before parsing — the user's source comes first, so its spans
// and line numbers are untouched — and everything below goes through the
// same checking and lowering as user code.

fn min(a: +i64, b: +i64) -> i64 {
    if a < b { a } else { b }
}

fn max(a: +i64, b: +i64) -> i64 {
    if a > b { a } else { b }
}

fn abs(n: +i64) -> i64 {
    if n < 0 { 0 - n } else { n }
}

// Compose a function with a continuation: the consumer that runs `f`, then
// jumps to `k`. `A → ⊥` is `-A`, so the lambda *is* that consumer.
fn then<A, B>(f: (A -> B), k: ↓-B) -> -A {
    fn(x: A) { f(x) @ ↑k }
}

// ── Consumer combinators ─────────────────────────────────────────────────
//
// A combinator that needs value inputs cannot be declared `<- A`: a negative
// fn's parameters form its continuation row, and values do not ride in a
// row. The stdlib shape for consumer combinators is therefore a positive fn
// returning the consumer, built as a λ whose body is a command — `A → ⊥`
// *is* `-A`.

// A tap: log a label and the value passing through, then forward it.
fn traced<T>(label: +String, k: ↓-T) -> -T {
    fn(x: T) { println(label); println(x); x @ ↑k }
}

// A failure consumer that discards the message and sends `fallback` onward
// — pairs with the `-String` outcomes of `parse_int`, `read_file`, and the
// other multi-outcome builtins.
fn defaulting<T>(fallback: T, k: ↓-T) -> -String {
    fn(m: +String) { fallback @ ↑k }
}

// ── Lists ────────────────────────────────────────────────────────────────
//
// A list is an ordinary recursive enum — nothing about it is built in.

enum List<T> {
    Nil,
    Cons(T, List<T>),
}

fn length<T>(xs: List<T>) -> i64 {
    match xs {
        Nil => 0,
        Cons(_, rest) => 1 + length(rest),
    }
}

fn append<T>(xs: List<T>, ys: List<T>) -> List<T> {
    match xs {
        Nil => ys,
        Cons(h, rest) => List::Cons(h, append(rest, ys)),
    }
}

fn map<A, B>(f: (A -> B), xs: List<A>) -> List<B> {
    match xs {
        Nil => List::Nil,
        Cons(h, rest) => List::Cons(f(h), map(f, rest)),
    }
}

// Indexing can find nothing, so it offers its outcomes to continuations,
// the way the lookup builtins do.
command nth<T>(xs: List<T>, i: +i64) | (found: -T, missing: -String) {
    match xs {
        Nil => "nothing at that index" @ missing,
        Cons(h, rest) => if i == 0 { h @ found } else { nth(rest, i - 1, found, missing) },
    }
}
