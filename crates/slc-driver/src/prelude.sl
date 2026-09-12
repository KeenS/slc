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
// names to the existing `(,)`/`1` and `⊥` units respectively.

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

// ── Consumer combinators ─────────────────────────────────────────────────
//
// A combinator that needs value inputs cannot be declared `<- A`: a negative
// fn's parameters form its continuation row, and values do not ride in a
// row. The stdlib shape for consumer combinators is therefore a positive fn
// returning the consumer, built as a λ whose body is a command — `A → ⊥`
// *is* `-A`.

// A tap: log a label and the value passing through, then forward it.
fn traced<T>(label: +String, k: -T) -> -T {
    fn(x: T) { label | println; x | println; x | k⟩ }
}

// A failure consumer that discards the message and sends `fallback` onward
// — pairs with the `-String` outcomes of `parse_int`, `read_file`, and the
// other multi-outcome builtins.
fn defaulting<T>(fallback: T, k: -T) -> -String {
    fn(m: +String) { fallback | k⟩ }
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
        Cons(_, rest) => 1 + (rest | length),
    }
}

fn append<T>(xs: List<T>, ys: List<T>) -> List<T> {
    match xs {
        Nil => ys,
        Cons(h, rest) => Cons(h, (rest, ys) | append),
    }
}

fn map<A, B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E} {
    match xs {
        Nil => Nil,
        Cons(h, rest) => Cons(h | f, (f, rest) | map),
    }
}

// Indexing can find nothing, so it offers its outcomes to continuations,
// the way the lookup builtins do.
command nth<T>(xs: List<T>, i: +i64) | (found: -T & missing: -String) {
    match xs {
        Nil => "nothing at that index" | missing⟩,
        Cons(h, rest) => {
            if i == 0 { h | found⟩ } else { (rest, i - 1) | nth | (found & missing)⟩ }
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
    fn fmt(self: +i64) -> String { self | int_to_str }
}

impl Display for String {
    fn fmt(self: +String) -> String { self }
}

impl Display for bool {
    fn fmt(self: +bool) -> String { if self { "true" } else { "false" } }
}

fn to_string<T: Display>(x: T) -> String { x | fmt }

fn fmt_items<T: Display>(xs: List<T>) -> String {
    match xs {
        Nil => "",
        Cons(h, Nil) => h | fmt,
        Cons(h, rest) => (h | fmt) + ", " + (rest | fmt_items),
    }
}

impl<T: Display> Display for List<T> {
    fn fmt(self: +List<T>) -> String { "[" + (self | fmt_items) + "]" }
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
        head <= x | head⟩,
        tail <= x | repeat | tail⟩,
    }
}

fn count_from(n: +i64) -> Stream<i64> {
    mu Stream {
        head <= n | head⟩,
        tail <= n + 1 | count_from | tail⟩,
    }
}

fn map_stream<A, B, E>(f: (A -> B / {..E}), s: Stream<A>) -> Stream<B> / {..E} {
    mu Stream {
        head <= s.head | f | head⟩,
        tail <= (f, s.tail) | map_stream | tail⟩,
    }
}

// The bridge back to data: the first `n` elements, as a list.
fn take<T>(s: Stream<T>, n: +i64) -> List<T> {
    if n <= 0 { Nil } else { Cons(s.head, (s.tail, n - 1) | take) }
}

// A one-item menu is a by-name thunk: `.force` re-runs its arm at every
// demand.
menu Lazy<T> {
    force: T,
}

// A stream is as often generated by a step function as by recursion.
// `unfold` is the general form — each step answers an element and the seed
// the rest is built from — and `iterate` the case where the state *is* the
// element. Only the demanded arm runs, so the step runs once per demand.

fn unfold<S, T>(step: (S -> (T ⊗ S)), seed: S) -> Stream<T> {
    mu Stream {
        head <= (seed | step).0 | head⟩,
        tail <= (step, (seed | step).1) | unfold | tail⟩,
    }
}

fn iterate<T>(f: (T -> T), x: T) -> Stream<T> {
    mu Stream {
        head <= x | head⟩,
        tail <= (f, x | f) | iterate | tail⟩,
    }
}

fn zip_stream<A, B>(a: Stream<A>, b: Stream<B>) -> Stream<(A ⊗ B)> {
    mu Stream {
        head <= (a.head, b.head) | head⟩,
        tail <= (a.tail, b.tail) | zip_stream | tail⟩,
    }
}

// Unlike the others this forces as it goes: `n` demands happen here rather
// than at the first demand of the result.
fn drop_stream<T>(s: Stream<T>, n: +i64) -> Stream<T> {
    if n <= 0 { s } else { (s.tail, n - 1) | drop_stream }
}

// ── Seq: the finite codata sequence ──────────────────────────────────────
//
// `List` is data, `Stream` is codata that never ends, and `Seq` is the one
// in between: a menu whose single item answers *whether* there is more. The
// recursion lives in the codata and the branching in the data, so a `Seq` is
// produced a step at a time and only as far as it is demanded — which is
// what neither neighbour can do. `filter_seq` over an infinite source
// terminates as long as something downstream stops asking:
//
//     ((odd, 1 | count_from | seq_of_stream) | filter_seq, 4) | take_seq
//
// There is no `impl Display for Seq`: showing one is `list_of_seq`, or
// `take_seq` first if it may not end.

// The step is named for its menu rather than for itself: a prelude type
// shadowed by a program strands the prelude functions that mention it, and
// `Step` is a name a program is likely to want.
enum SeqStep<T> {
    Done,
    Yield(T, Seq<T>),
}

menu Seq<T> {
    next: SeqStep<T>,
}

fn seq_of_list<T>(xs: List<T>) -> Seq<T> {
    mu Seq {
        next <= match xs {
            Nil => SeqStep::Done | next⟩,
            Cons(h, rest) => SeqStep::Yield(h, rest | seq_of_list) | next⟩,
        },
    }
}

// The bridge back to data, as `take` is for `Stream`. A `Seq` that never
// answers `Done` does not come back; `take_seq` it first.
fn list_of_seq<T>(s: Seq<T>) -> List<T> {
    match s.next {
        SeqStep::Done => Nil,
        SeqStep::Yield(h, rest) => Cons(h, rest | list_of_seq),
    }
}

// Every stream is a sequence that never ends.
fn seq_of_stream<T>(s: Stream<T>) -> Seq<T> {
    mu Seq {
        next <= SeqStep::Yield(s.head, s.tail | seq_of_stream) | next⟩,
    }
}

fn map_seq<A, B, E>(f: (A -> B / {..E}), s: Seq<A>) -> Seq<B> / {..E} {
    mu Seq {
        next <= match s.next {
            SeqStep::Done => SeqStep::Done | next⟩,
            SeqStep::Yield(h, rest) => SeqStep::Yield(h | f, (f, rest) | map_seq) | next⟩,
        },
    }
}

// A dropped element is not a step of the result, so the arm demands the
// rest itself rather than answering — the loop lives in the demand.
fn filter_seq<T, E>(keep: (T -> bool / {..E}), s: Seq<T>) -> Seq<T> / {..E} {
    mu Seq {
        next <= match s.next {
            SeqStep::Done => SeqStep::Done | next⟩,
            SeqStep::Yield(h, rest) => if h | keep {
                SeqStep::Yield(h, (keep, rest) | filter_seq) | next⟩
            } else {
                ((keep, rest) | filter_seq).next | next⟩
            },
        },
    }
}

fn take_seq<T>(s: Seq<T>, n: +i64) -> Seq<T> {
    mu Seq {
        next <= if n <= 0 {
            SeqStep::Done | next⟩
        } else {
            match s.next {
                SeqStep::Done => SeqStep::Done | next⟩,
                SeqStep::Yield(h, rest) => SeqStep::Yield(h, (rest, n - 1) | take_seq) | next⟩,
            }
        },
    }
}

// The other bridge: a stream, cut where a value stops passing. The result
// can end, so it is a `Seq` — the type says what the function does.
fn take_while<T, E>(keep: (T -> bool / {..E}), s: Stream<T>) -> Seq<T> / {..E} {
    mu Seq {
        next <= if s.head | keep {
            SeqStep::Yield(s.head, (keep, s.tail) | take_while) | next⟩
        } else {
            SeqStep::Done | next⟩
        },
    }
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
