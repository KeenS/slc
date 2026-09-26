// `seq`: the finite codata sequence.
//
// `List` is data, `Stream` is codata that never ends, and `Seq` is the one
// in between: a menu whose single item answers *whether* there is more. The
// recursion lives in the codata and the branching in the data, so a `Seq` is
// produced a step at a time and only as far as it is demanded — which is
// what neither neighbour can do. `seq::filter` over an infinite source
// terminates as long as something downstream stops asking:
//
//     ((odd, 1 | stream::count_from | seq::of_stream) | seq::filter, 4) | seq::take
//
// A `Seq` carries a row, `Seq<T, ..E>`: what demanding its steps performs.
// Building one performs nothing, so `seq::map` with an effectful function
// answers at once, and the effects happen where the steps are demanded.
// `Seq<T>` is a sequence whose steps perform nothing.
//
// There is no `impl Display for Seq`: showing one is `seq::to_list`, or
// `seq::take` first if it may not end.
//
// The module carries the context, so nothing here repeats it: `seq::map`,
// not `map_seq`.

cite list::List::*;
cite stream::Stream;

pub enum Step<+T, E> {
    Done,
    Yield(T, Seq<T, ..E>),
}

pub menu Seq<+T, E> / {..E} {
    next: Step<T, ..E>,
}

pub func of_list<+T>(xs: list::List<T>) -> Seq<T> {
    mu Seq {
        next <= of xs {
            Nil => <Step::Done | next>,
            Cons(h, rest) => <Step::Yield(h, <rest | of_list) | next>,
        },
    }
}

// The bridge back to data, as `stream::take` is for `Stream`. A `Seq`
// that never answers `Done` does not come back; `take` it first.
pub func to_list<+T, E>(s: Seq<T, ..E>) -> list::List<T> / {..E} {
    of s.next {
        Step::Done => Nil,
        Step::Yield(h, rest) => Cons(h, <rest | to_list),
    }
}

// Every stream is a sequence that never ends.
pub func of_stream<+T, E>(s: (-> Stream<T, ..E> / {..E})) -> Seq<T, ..E> {
    mu Seq {
        next <= <Step::Yield(s.head, <s.tail | of_stream) | next>,
    }
}

pub func map<+A, +B, E>(f: (A -> B / {..E}), s: Seq<A, ..E>) -> Seq<B, ..E> {
    mu Seq {
        next <= of s.next {
            Step::Done => <Step::Done | next>,
            Step::Yield(h, rest) => <Step::Yield(<h | f, <(f, rest) | map) | next>,
        },
    }
}

// A dropped element is not a step of the result, so the arm demands
// the rest itself rather than answering — the loop lives in the demand.
pub func filter<+T, E>(keep: (T -> Bool / {..E}), s: Seq<T, ..E>) -> Seq<T, ..E> {
    mu Seq {
        next <= of s.next {
            Step::Done => <Step::Done | next>,
            Step::Yield(h, rest) => of <h | keep {
                True => <Step::Yield(h, <(keep, rest) | filter) | next>,
                _ => <(<(keep, rest) | filter).next | next>,
            },
        },
    }
}

pub func take<+T, E>(s: Seq<T, ..E>, n: i64) -> Seq<T, ..E> {
    mu Seq {
        next <= of (<(n, 0) | le) {
            True => {
                <Step::Done | next>
            },
            _ => {
                of s.next {
                    Step::Done => <Step::Done | next>,
                    Step::Yield(h, rest) => <Step::Yield(h, <(rest, <(n, 1) | sub) | take) | next>,
                }
            },
        },
    }
}

// The other bridge: a stream, cut where a value stops passing. The
// result can end, so it is a `Seq` — the type says what the function
// does.
pub func take_while<+T, E>(
    keep: (T -> Bool / {..E}),
    s: (-> Stream<T, ..E> / {..E}),
) -> Seq<T, ..E> {
    mu Seq {
        next <= of <s.head | keep {
            True => <Step::Yield(s.head, <(keep, s.tail) | take_while) | next>,
            _ => <Step::Done | next>,
        },
    }
}
