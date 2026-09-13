// `seq`: the finite codata sequence.
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

mod seq {
    use list::List::*;
    use stream::Stream;

    // The step is named for its menu rather than for itself: a library
    // type shadowed by a program strands the library functions that mention
    // it, and `Step` is a name a program is likely to want.
    pub enum SeqStep<T> {
        Done,
        Yield(T, Seq<T>),
    }

    pub menu Seq<T> {
        next: SeqStep<T>,
    }

    pub fn seq_of_list<T>(xs: list::List<T>) -> Seq<T> {
        mu Seq {
            next <= match xs {
                Nil => SeqStep::Done | next⟩,
                Cons(h, rest) => SeqStep::Yield(h, rest | seq_of_list) | next⟩,
            },
        }
    }

    // The bridge back to data, as `take` is for `Stream`. A `Seq` that
    // never answers `Done` does not come back; `take_seq` it first.
    pub fn list_of_seq<T>(s: Seq<T>) -> list::List<T> {
        match s.next {
            SeqStep::Done => Nil,
            SeqStep::Yield(h, rest) => Cons(h, rest | list_of_seq),
        }
    }

    // Every stream is a sequence that never ends.
    pub fn seq_of_stream<T>(s: Stream<T>) -> Seq<T> {
        mu Seq {
            next <= SeqStep::Yield(s.head, s.tail | seq_of_stream) | next⟩,
        }
    }

    pub fn map_seq<A, B, E>(f: (A -> B / {..E}), s: Seq<A>) -> Seq<B> / {..E} {
        mu Seq {
            next <= match s.next {
                SeqStep::Done => SeqStep::Done | next⟩,
                SeqStep::Yield(h, rest) => SeqStep::Yield(h | f, (f, rest) | map_seq) | next⟩,
            },
        }
    }

    // A dropped element is not a step of the result, so the arm demands
    // the rest itself rather than answering — the loop lives in the demand.
    pub fn filter_seq<T, E>(keep: (T -> bool / {..E}), s: Seq<T>) -> Seq<T> / {..E} {
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

    pub fn take_seq<T>(s: Seq<T>, n: i64) -> Seq<T> {
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

    // The other bridge: a stream, cut where a value stops passing. The
    // result can end, so it is a `Seq` — the type says what the function
    // does.
    pub fn take_while<T, E>(keep: (T -> bool / {..E}), s: Stream<T>) -> Seq<T> / {..E} {
        mu Seq {
            next <= if s.head | keep {
                SeqStep::Yield(s.head, (keep, s.tail) | take_while) | next⟩
            } else {
                SeqStep::Done | next⟩
            },
        }
    }
}
