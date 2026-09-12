// The Slant prelude: ordinary declarations, available to every program.
//
// The driver appends this file to the program before parsing — the user's
// source comes first, so its spans and line numbers are untouched — and
// everything below goes through the same checking and lowering as user code.
// The only compiler trick is that the exact nullary Unit and Bottom
// declarations below are aliases for the existing multiplicative units.

// ── Logical units ───────────────────────────────────────────────────────
//
// Empty and Top remain ordinary nominal declarations. Unit and Bottom give
// names to the existing `()`/`1` and `⊥` units respectively.

data Unit {}
form Bottom {}
enum Empty {}
menu Top {}

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
fn then<A, B, E>(f: (A -> B / {..E}), k: -B) -> -A / {..E} {
    fn(x: A) { f(x) @ k }
}

// ── Consumer combinators ─────────────────────────────────────────────────
//
// A combinator that needs value inputs cannot be declared `<- A`: a negative
// fn's parameters form its continuation row, and values do not ride in a
// row. The stdlib shape for consumer combinators is therefore a positive fn
// returning the consumer, built as a λ whose body is a command — `A → ⊥`
// *is* `-A`.

// A tap: log a label and the value passing through, then forward it.
fn traced<T>(label: +String, k: -T) -> -T {
    fn(x: T) { println(label); println(x); x @ k }
}

// A failure consumer that discards the message and sends `fallback` onward
// — pairs with the `-String` outcomes of `parse_int`, `read_file`, and the
// other multi-outcome builtins.
fn defaulting<T>(fallback: T, k: -T) -> -String {
    fn(m: +String) { fallback @ k }
}

// ── Lists ────────────────────────────────────────────────────────────────
//
// A list is an ordinary recursive enum — nothing about it is built in.

enum List<T> {
    Nil,
    Cons(T, List<T>),
}

// The import pins `Nil` and `Cons` to List *within the prelude*: imports
// are scoped to their source unit, so a program's own `Nil` — or its own
// glob — never changes what these mean, and theirs is untouched by ours.
use List::*;

fn length<T>(xs: List<T>) -> i64 {
    match xs {
        Nil => 0,
        Cons(_, rest) => 1 + length(rest),
    }
}

fn append<T>(xs: List<T>, ys: List<T>) -> List<T> {
    match xs {
        Nil => ys,
        Cons(h, rest) => Cons(h, append(rest, ys)),
    }
}

fn map<A, B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E} {
    match xs {
        Nil => Nil,
        Cons(h, rest) => Cons(f(h), map(f, rest)),
    }
}

// Indexing can find nothing, so it offers its outcomes to continuations,
// the way the lookup builtins do.
command nth<T>(xs: List<T>, i: +i64) | (found: -T, missing: -String) {
    match xs {
        Nil => "nothing at that index" @ missing,
        Cons(h, rest) => {
            if i == 0 { h @ found } else { nth(rest, i - 1, found, missing) }
        },
    }
}

// ── Display ──────────────────────────────────────────────────────────────
//
// User-facing formatting, as in Rust: `fmt` renders a value as the String a
// person should see — `fmt("hi")` is `hi`, unquoted — and `to_string` is
// the same act as a plain function.

trait Display {
    fn fmt(self: +Self) -> String;
}

impl Display for i64 {
    fn fmt(self: +i64) -> String { int_to_str(self) }
}

impl Display for String {
    fn fmt(self: +String) -> String { self }
}

impl Display for bool {
    fn fmt(self: +bool) -> String { if self { "true" } else { "false" } }
}

fn to_string<T: Display>(x: T) -> String { fmt(x) }

fn fmt_items<T: Display>(xs: List<T>) -> String {
    match xs {
        Nil => "",
        Cons(h, Nil) => fmt(h),
        Cons(h, rest) => fmt(h) + ", " + fmt_items(rest),
    }
}

impl<T: Display> Display for List<T> {
    fn fmt(self: +List<T>) -> String { "[" + fmt_items(self) + "]" }
}

// ── The negative side: Stream and Lazy ───────────────────────────────────
//
// A menu is codata: only the demanded branch ever runs, so an infinite
// structure is just a menu that offers itself again. `Stream` is the
// coinductive mirror of `List`. There is no `impl Display for Stream` — an
// infinite structure cannot print whole; the honest form is
// `fmt(take(s, n))`.

menu Stream<T> {
    head: T,
    tail: Stream<T>,
}

fn repeat<T>(x: T) -> Stream<T> {
    mu Stream {
        head <= x @ head,
        tail <= repeat(x) @ tail,
    }
}

fn count_from(n: +i64) -> Stream<i64> {
    mu Stream {
        head <= n @ head,
        tail <= count_from(n + 1) @ tail,
    }
}

fn map_stream<A, B, E>(f: (A -> B / {..E}), s: Stream<A>) -> Stream<B> / {..E} {
    mu Stream {
        head <= f(s.head) @ head,
        tail <= map_stream(f, s.tail) @ tail,
    }
}

// The bridge back to data: the first `n` elements, as a list.
fn take<T>(s: Stream<T>, n: +i64) -> List<T> {
    if n <= 0 { Nil } else { Cons(s.head, take(s.tail, n - 1)) }
}

// A one-item menu is a by-name thunk: `.force` re-runs its arm at every
// demand.
menu Lazy<T> {
    force: T,
}

// ── Option and Result ────────────────────────────────────────────────────
//
// Either/or outcomes are *additive* — one variant, not every field — so
// they are enums, and their consumers are `select`s over them. (A `form`
// would be the wrong connective: it wants every field at once.)

enum Option<T> {
    None,
    Some(T),
}

enum Result<T, E> {
    Ok(T),
    Err(E),
}

fn unwrap_or<T>(o: Option<T>, fallback: T) -> T {
    match o {
        Option::None => fallback,
        Option::Some(x) => x,
    }
}
