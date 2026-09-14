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
// There is no `impl Display for Seq`: showing one is `seq::to_list`, or
// `seq::take` first if it may not end.
//
// The module carries the context, so nothing here repeats it: `seq::map`,
// not `map_seq`.

mod seq {
    use list::List::*;
    use stream::Stream;

    pub enum Step<+T> {
        Done,
        Yield(T, Seq<T>),
    }

    pub menu Seq<+T> {
        next: Step<T>,
    }

    pub fn of_list<+T>(xs: list::List<T>) -> Seq<T> {
        mu Seq {
            next <= match xs {
                Nil => <Step::Done | next>,
                Cons(h, rest) => <Step::Yield(h, <rest | of_list) | next>,
            },
        }
    }

    // The bridge back to data, as `stream::take` is for `Stream`. A `Seq`
    // that never answers `Done` does not come back; `take` it first.
    pub fn to_list<+T>(s: Seq<T>) -> list::List<T> {
        match s.next {
            Step::Done => Nil,
            Step::Yield(h, rest) => Cons(h, <rest | to_list),
        }
    }

    // Every stream is a sequence that never ends.
    pub fn of_stream<+T>(s: Stream<T>) -> Seq<T> {
        mu Seq {
            next <= <Step::Yield(s.head, <s.tail | of_stream) | next>,
        }
    }

    pub fn map<+A, +B, E>(f: (A -> B / {..E}), s: Seq<A>) -> Seq<B> / {..E} {
        mu Seq {
            next <= match s.next {
                Step::Done => <Step::Done | next>,
                // The rest is built with `let+` before it is stored: a stored
                // computation may not perform anything, and though building a
                // `Seq` performs nothing, the call's row says `..E`.
                Step::Yield(h, rest) => {
                    let+ tail = <(f, rest) | map;
                    <Step::Yield(<h | f, tail) | next>
                },
            },
        }
    }

    // A dropped element is not a step of the result, so the arm demands
    // the rest itself rather than answering — the loop lives in the demand.
    pub fn filter<+T, E>(keep: (T -> Bool / {..E}), s: Seq<T>) -> Seq<T> / {..E} {
        mu Seq {
            next <= match s.next {
                Step::Done => <Step::Done | next>,
                Step::Yield(h, rest) => match <h | keep { True => {
                    let+ tail = <(keep, rest) | filter;
                    <Step::Yield(h, tail) | next>
                }, _ => {
                    <(<(keep, rest) | filter).next | next>
                } },
            },
        }
    }

    pub fn take<+T>(s: Seq<T>, n: i64) -> Seq<T> {
        mu Seq {
            next <= match (<(n, 0) | le) { True => {
                <Step::Done | next>
            }, _ => {
                match s.next {
                    Step::Done => <Step::Done | next>,
                    Step::Yield(h, rest) => <Step::Yield(h, <(rest, (<(n, 1) | sub)) | take) | next>,
                }
            } },
        }
    }

    // The other bridge: a stream, cut where a value stops passing. The
    // result can end, so it is a `Seq` — the type says what the function
    // does.
    pub fn take_while<+T, E>(keep: (T -> Bool / {..E}), s: Stream<T>) -> Seq<T> / {..E} {
        mu Seq {
            next <= match <s.head | keep { True => {
                let+ tail = <(keep, s.tail) | take_while;
                <Step::Yield(s.head, tail) | next>
            }, _ => {
                <Step::Done | next>
            } },
        }
    }
}
