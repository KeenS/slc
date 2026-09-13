// `Seq` — the finite codata sequence, and what it buys.
//
// The stdlib has both neighbours: `List` is data, and `Stream` is codata that
// never ends. `Seq` is the one in between —
//
//     enum Step<+T> { Done, Yield(T, Seq<T>) }
//     menu Seq<+T> { next: Step<T> }
//
// — a menu whose single item answers *whether* there is more. The recursion
// lives in the codata and the branching in the data, so only the step that
// is demanded ever runs. That is the whole point: a pipeline over a `Seq`
// does no work the consumer does not ask for, which is what lets `filter`
// run over an infinite source.
//
// Everything is called through its module — `seq::map`, `stream::take` —
// so the two `take`s, the two `map`s and the list's own never meet.

use list::List::*;

fn odd(n: i64) -> bool { n % 2 == 1 }
fn double(n: i64) -> i64 { n * 2 }
fn under_ten(n: i64) -> bool { n < 10 }

// A step function is another way to write a stream: each step answers an
// element and the seed the rest is built from.
fn halving(n: i64) -> (i64, i64) {
    (n, n / 2)
}

command main | (exit: i32) / {IO} {
    let xs = Cons(1, Cons(2, Cons(3, Cons(4, Cons(5, Nil)))));

    // Over data, `Seq` is an ordinary lazy pipeline: nothing runs until
    // `seq::to_list` asks, and then only as far as it asks. A stage that takes
    // more than what flows in names it, `s => (odd, s)`, and the chain stays flat.
    ⟨xs | seq::of_list | s => (odd, s) | seq::filter | s => (double, s) | seq::map
        | seq::to_list | fmt | println;                     // "[2, 6, 10]"

    // Over an infinite source it is the same program. `seq::filter` has no
    // idea the stream never ends, and `seq::take` is what stops it: four
    // answers demanded, four produced.
    let naturals = ⟨1 | stream::count_from;
    ⟨naturals | seq::of_stream | s => (odd, s) | seq::filter | s => (s, 4) | seq::take
        | seq::to_list | fmt | println;                     // "[1, 3, 5, 7]"

    // `seq::take_while` is the other bridge — a stream cut where a value
    // stops passing. The result can end, so its type is `Seq`, not `Stream`.
    ⟨(under_ten, naturals) | seq::take_while | seq::to_list | fmt | println;

    // `stream::unfold` generates; `stream::zip` and `stream::drop` rearrange.
    ⟨(halving, 64) | stream::unfold | s => (s, 5) | stream::take | fmt | println;   // "[64, 32, 16, 8, 4]"
    ⟨(naturals, 3) | stream::drop | s => (s, 3) | stream::take | fmt | println;     // "[4, 5, 6]"
    ⟨(double, 1) | stream::iterate | s => (s, 5) | stream::take | fmt | println;    // "[1, 2, 4, 8, 16]"

    ⟨0 | exit⟩
}
