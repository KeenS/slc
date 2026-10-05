// `list`: a list is an ordinary recursive enum — nothing about it is built in.
//
// `cite list::List::*;` brings `Nil` and `Cons` in bare; the functions are
// reached as `list::length`, or imported one by one.

pub enum List<+T> {
    Nil,
    Cons(T, List<T>),
}

// The import pins `Nil` and `Cons` to List *within this unit*: imports
// are scoped to their source unit, so a program's own `Nil` — or its own
// glob — never changes what these mean, and theirs is untouched by ours.
cite List::*;

pub func length<+T>(xs: List<T>) -> i64 {
    of xs {
        Nil => 0,
        Cons(_, rest) => (<(1, <rest | length) | add),
    }
}

pub func append<+T>(xs: List<T>, ys: List<T>) -> List<T> {
    of xs {
        Nil => ys,
        Cons(h, rest) => Cons(h, <(rest, ys) | append),
    }
}

pub func map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E} {
    of xs {
        Nil => Nil,
        Cons(h, rest) => Cons(<h | f, <(f, rest) | map),
    }
}

// Indexing can find nothing, so it offers its outcomes to continuations,
// the way the lookup builtins do.
pub proc nth<+T, E>(xs: List<T>, i: i64) | (
    found: (-T / {..E})
    & missing: (-String / {..E})
) / {..E} {
    of xs {
        Nil => <"nothing at that index" | missing>,
        Cons(h, rest) => {
            of (<(i, 0) | eq) {
                True => { <h | found> },
                _ => { <(rest, <(i, 1) | sub) | nth | (found & missing)> },
            }
        },
    }
}

// Inclusive. Empty when `from` is past `to`. A one-element range does not
// add, so the greatest `i64` is a range of itself.
pub func range(from: i64, to: i64) -> List<i64> {
    of (<(from, to) | gt) {
        True => Nil,
        _ => of (<(from, to) | eq) {
            True => Cons(from, Nil),
            _ => Cons(from, <(<(from, 1) | add, to) | range),
        },
    }
}

pub func filter<+T, E>(keep: (T -> Bool / {..E}), xs: List<T>) -> List<T> / {..E} {
    of xs {
        Nil => Nil,
        Cons(h, rest) => of (<h | keep) {
            True => Cons(h, <(keep, rest) | filter),
            _ => <(keep, rest) | filter,
        },
    }
}

pub func fold<+A, +B, E>(xs: List<A>, init: B, f: ((B, A) -> B / {..E})) -> B / {..E} {
    of xs {
        Nil => init,
        Cons(h, rest) => {
            let next = <(init, h) | f;
            <(rest, next, f) | fold
        },
    }
}

func add_i64(acc: i64, n: i64) -> i64 {
    <(acc, n) | add
}

pub func sum(xs: List<i64>) -> i64 {
    <(xs, 0, add_i64) | fold
}

func rev_onto<+T>(xs: List<T>, acc: List<T>) -> List<T> {
    of xs {
        Nil => acc,
        Cons(h, rest) => <(rest, Cons(h, acc)) | rev_onto,
    }
}

pub func reverse<+T>(xs: List<T>) -> List<T> {
    <(xs, Nil) | rev_onto
}

pub func take<+T>(xs: List<T>, n: i64) -> List<T> {
    of (<(n, 0) | le) {
        True => Nil,
        _ => of xs {
            Nil => Nil,
            Cons(h, rest) => Cons(h, <(rest, <(n, 1) | sub) | take),
        },
    }
}

pub func drop<+T>(xs: List<T>, n: i64) -> List<T> {
    of (<(n, 0) | le) {
        True => xs,
        _ => of xs {
            Nil => Nil,
            Cons(_, rest) => <(rest, <(n, 1) | sub) | drop,
        },
    }
}

func fmt_items<+T: Display>(xs: List<T>) -> String {
    of xs {
        Nil => "",
        Cons(h, Nil) => <h | fmt,
        Cons(h, rest) => (<(<h | fmt, ", ") | add | x => (x, <rest | fmt_items) | add),
    }
}

impl<+T: Display> Display for List<T> {
    func fmt(self: List<T>) -> String { (<("[", <self | fmt_items) | add | x => (x, "]") | add) }
}
