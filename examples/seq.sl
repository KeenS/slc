// `Seq` — the finite codata sequence, and what it buys.
//
// The prelude has both neighbours already: `List` is data, and `Stream` is
// codata that never ends. `Seq` is the one in between —
//
//     enum SeqStep<T> { Done, Yield(T, Seq<T>) }
//     menu Seq<T> { next: SeqStep<T> }
//
// — a menu whose single item answers *whether* there is more. The recursion
// lives in the codata and the branching in the data, so only the step that
// is demanded ever runs. That is the whole point: a pipeline over a `Seq`
// does no work the consumer does not ask for, which is what lets `filter`
// run over an infinite source.

fn odd(n: i64) -> bool { n % 2 == 1 }
fn double(n: i64) -> i64 { n * 2 }
fn under_ten(n: i64) -> bool { n < 10 }

// A step function is another way to write a stream: each step answers an
// element and the seed the rest is built from.
fn halving(n: i64) -> (i64 ⊗ i64) {
    (n, n / 2)
}

command main | (exit: i32) / {IO} {
    let xs = Cons(1, Cons(2, Cons(3, Cons(4, Cons(5, Nil)))));

    // Over data, `Seq` is an ordinary lazy pipeline: nothing runs until
    // `list_of_seq` asks, and then only as far as it asks.
    (double, (odd, xs | seq_of_list) | filter_seq) | map_seq
        | list_of_seq | fmt | println;                      // "[2, 6, 10]"

    // Over an infinite source it is the same program. `filter_seq` has no
    // idea the stream never ends, and `take_seq` is what stops it: four
    // answers demanded, four produced.
    let naturals = 1 | count_from;
    ((odd, naturals | seq_of_stream) | filter_seq, 4) | take_seq
        | list_of_seq | fmt | println;                      // "[1, 3, 5, 7]"

    // `take_while` is the other bridge — a stream cut where a value stops
    // passing. The result can end, so its type is `Seq`, not `Stream`.
    (under_ten, naturals) | take_while | list_of_seq | fmt | println;

    // `unfold` generates; `zip_stream` and `drop_stream` rearrange.
    ((halving, 64) | unfold, 5) | take | fmt | println;      // "[64, 32, 16, 8, 4]"
    ((naturals, 3) | drop_stream, 3) | take | fmt | println; // "[4, 5, 6]"
    ((double, 1) | iterate, 5) | take | fmt | println;       // "[1, 2, 4, 8, 16]"

    0 | exit⟩
}
