// `array`: an immutable array, and the node it is built from.
//
// `Array<T>` is a 4-way trie. `Array4<T>` is one branch: one to four slots,
// every slot occupied. A computed index is a match scanned from the first
// arm, so the branch stays four wide rather than thirty-two. The digits of
// an index in base four select the path. `push` and `update` answer a new
// array and leave the one they were given unchanged. `get` and `update` can
// miss, so each offers that outcome to a continuation, the way `list::nth`
// does.

use list::List;
use list::List::*;

pub enum Array4<+T> {
    One(T),
    Two(T, T),
    Three(T, T, T),
    Four(T, T, T, T),
}

pub func slots<+T>(a: Array4<T>) -> i64 {
    of a {
        Array4::One(_) => 1,
        Array4::Two(_, _) => 2,
        Array4::Three(_, _, _) => 3,
        Array4::Four(_, _, _, _) => 4,
    }
}

func at<+T>(a: Array4<T>, i: i64) -> T {
    of a {
        Array4::One(a0) => a0,
        Array4::Two(a0, a1) => of i {
            0 => a0,
            _ => a1,
        },
        Array4::Three(a0, a1, a2) => of i {
            0 => a0,
            1 => a1,
            _ => a2,
        },
        Array4::Four(a0, a1, a2, a3) => of i {
            0 => a0,
            1 => a1,
            2 => a2,
            _ => a3,
        },
    }
}

func put<+T>(a: Array4<T>, i: i64, value: T) -> Array4<T> {
    of a {
        Array4::One(_) => Array4::One(value),
        Array4::Two(a0, a1) => of i {
            0 => Array4::Two(value, a1),
            _ => Array4::Two(a0, value),
        },
        Array4::Three(a0, a1, a2) => of i {
            0 => Array4::Three(value, a1, a2),
            1 => Array4::Three(a0, value, a2),
            _ => Array4::Three(a0, a1, value),
        },
        Array4::Four(a0, a1, a2, a3) => of i {
            0 => Array4::Four(value, a1, a2, a3),
            1 => Array4::Four(a0, value, a2, a3),
            2 => Array4::Four(a0, a1, value, a3),
            _ => Array4::Four(a0, a1, a2, value),
        },
    }
}

func snoc_slot<+T>(a: Array4<T>, value: T) -> Array4<T> {
    of a {
        Array4::One(a0) => Array4::Two(a0, value),
        Array4::Two(a0, a1) => Array4::Three(a0, a1, value),
        Array4::Three(a0, a1, a2) => Array4::Four(a0, a1, a2, value),
        Array4::Four(a0, a1, a2, a3) => Array4::Four(a0, a1, a2, a3),
    }
}

pub proc slot_get<+T, E>(a: Array4<T>, i: i64) | (
    found: (-T / {..E})
    & missing: (-String / {..E})
) / {..E} {
    of (<(i, 0) | lt) {
        True => <"nothing at that index" | missing>,
        _ => of (<(i, <a | slots) | lt) {
            True => <(<(a, i) | at) | found>,
            _ => <"nothing at that index" | missing>,
        },
    }
}

pub proc slot_update<+T, E>(a: Array4<T>, i: i64, value: T) | (
    updated: (-Array4<T> / {..E})
    & missing: (-String / {..E})
) / {..E} {
    of (<(i, 0) | lt) {
        True => <"nothing at that index" | missing>,
        _ => of (<(i, <a | slots) | lt) {
            True => <(<(a, i, value) | put) | updated>,
            _ => <"nothing at that index" | missing>,
        },
    }
}

func wrap(body: String) -> String {
    (<("[", body) | add | x => (x, "]") | add)
}

func join(left: String, right: String) -> String {
    (<(left, ", ") | add | x => (x, right) | add)
}

impl<+T: Display> Display for Array4<T> {
    func fmt(self: Array4<T>) -> String {
        of self {
            Array4::One(a) => <(<a | fmt) | wrap,
            Array4::Two(a, b) => <(<(<a | fmt, <b | fmt) | join) | wrap,
            Array4::Three(a, b, c) => <(<(<(<a | fmt, <b | fmt) | join, <c | fmt) | join) | wrap,
            Array4::Four(a, b, c, d) => {
                <(<(<(<(<a | fmt, <b | fmt) | join, <c | fmt) | join, <d | fmt) | join) | wrap
            },
        }
    }
}

// A branch of the trie. A bucket holds elements. A node holds child tries of
// one height, filled from the left: every child but the last is full.
enum Trie<+T> {
    Bucket(Array4<T>),
    Node(Array4<Trie<T>>),
}

// `Fits` is the same height with the element added. `Overflow` is a new
// sibling of this trie, holding only the element that did not fit.
enum Grow<+T> {
    Fits(Trie<T>),
    Overflow(Trie<T>),
}

pub enum Array<+T> {
    Empty,
    Tree(i64, Trie<T>),
}

pub func empty<+T>() -> Array<T> {
    Array::Empty
}

pub func length<+T>(a: Array<T>) -> i64 {
    of a {
        Array::Empty => 0,
        Array::Tree(n, _) => n,
    }
}

// The smallest power of four that can hold `n` elements, and at least four.
func root_span(n: i64, c: i64) -> i64 {
    of (<(c, n) | lt) {
        True => <(n, <(c, 4) | mul) | root_span,
        _ => c,
    }
}

func trie_at<+T>(trie: Trie<T>, i: i64, span: i64) -> T {
    of trie {
        Trie::Bucket(bucket) => <(bucket, i) | at,
        Trie::Node(children) => {
            let child_span = <(span, 4) | div;
            let digit = <(i, child_span) | div;
            let rest = <(i, child_span) | rem;
            let child = <(children, digit) | at;
            <(child, rest, child_span) | trie_at
        },
    }
}

func trie_put<+T>(trie: Trie<T>, i: i64, span: i64, value: T) -> Trie<T> {
    of trie {
        Trie::Bucket(bucket) => Trie::Bucket(<(bucket, i, value) | put),
        Trie::Node(children) => {
            let child_span = <(span, 4) | div;
            let digit = <(i, child_span) | div;
            let rest = <(i, child_span) | rem;
            let child = <(<(children, digit) | at, rest, child_span, value) | trie_put;
            Trie::Node(<(children, digit, child) | put)
        },
    }
}

func push_trie<+T>(trie: Trie<T>, span: i64, value: T) -> Grow<T> {
    of trie {
        Trie::Bucket(bucket) => of (<(<bucket | slots, 4) | lt) {
            True => Grow::Fits(Trie::Bucket(<(bucket, value) | snoc_slot)),
            _ => Grow::Overflow(Trie::Bucket(Array4::One(value))),
        },
        Trie::Node(children) => {
            let child_span = <(span, 4) | div;
            let ix = <(<children | slots, 1) | sub;
            of <(<(children, ix) | at, child_span, value) | push_trie {
                Grow::Fits(child) => Grow::Fits(Trie::Node(<(children, ix, child) | put)),
                Grow::Overflow(extra) => of (<(<children | slots, 4) | lt) {
                    True => Grow::Fits(Trie::Node(<(children, extra) | snoc_slot)),
                    _ => Grow::Overflow(Trie::Node(Array4::One(extra))),
                },
            }
        },
    }
}

pub func push<+T>(a: Array<T>, value: T) -> Array<T> {
    of a {
        Array::Empty => Array::Tree(1, Trie::Bucket(Array4::One(value))),
        Array::Tree(n, root) => of <(root, <(n, 4) | root_span, value) | push_trie {
            Grow::Fits(root) => Array::Tree(<(n, 1) | add, root),
            Grow::Overflow(extra) => Array::Tree(
                <(n, 1) | add,
                Trie::Node(Array4::Two(root, extra)),
            ),
        },
    }
}

func within(i: i64, n: i64) -> Bool {
    of (<(i, 0) | lt) {
        True => False,
        _ => <(i, n) | lt,
    }
}

pub proc get<+T, E>(a: Array<T>, i: i64) | (
    found: (-T / {..E})
    & missing: (-String / {..E})
) / {..E} {
    of a {
        Array::Empty => <"nothing at that index" | missing>,
        Array::Tree(n, root) => of (<(i, n) | within) {
            True => <(<(root, i, <(n, 4) | root_span) | trie_at) | found>,
            _ => <"nothing at that index" | missing>,
        },
    }
}

pub proc update<+T, E>(a: Array<T>, i: i64, value: T) | (
    updated: (-Array<T> / {..E})
    & missing: (-String / {..E})
) / {..E} {
    of a {
        Array::Empty => <"nothing at that index" | missing>,
        Array::Tree(n, root) => of (<(i, n) | within) {
            True => {
                <Array::Tree(n, <(root, i, <(n, 4) | root_span, value) | trie_put) | updated>
            },
            _ => <"nothing at that index" | missing>,
        },
    }
}

func bucket_onto<+T>(bucket: Array4<T>, tail: List<T>) -> List<T> {
    of bucket {
        Array4::One(a) => Cons(a, tail),
        Array4::Two(a, b) => Cons(a, Cons(b, tail)),
        Array4::Three(a, b, c) => Cons(a, Cons(b, Cons(c, tail))),
        Array4::Four(a, b, c, d) => Cons(a, Cons(b, Cons(c, Cons(d, tail)))),
    }
}

func trie_onto<+T>(trie: Trie<T>, tail: List<T>) -> List<T> {
    of trie {
        Trie::Bucket(bucket) => <(bucket, tail) | bucket_onto,
        Trie::Node(children) => of children {
            Array4::One(a) => <(a, tail) | trie_onto,
            Array4::Two(a, b) => <(a, <(b, tail) | trie_onto) | trie_onto,
            Array4::Three(a, b, c) => <(a, <(b, <(c, tail) | trie_onto) | trie_onto) | trie_onto,
            Array4::Four(a, b, c, d) => {
                <(a, <(b, <(c, <(d, tail) | trie_onto) | trie_onto) | trie_onto) | trie_onto
            },
        },
    }
}

pub func to_list<+T>(a: Array<T>) -> List<T> {
    of a {
        Array::Empty => Nil,
        Array::Tree(_, root) => <(root, Nil) | trie_onto,
    }
}

func push_all<+T>(xs: List<T>, a: Array<T>) -> Array<T> {
    of xs {
        Nil => a,
        Cons(h, rest) => <(rest, <(a, h) | push) | push_all,
    }
}

pub func of_list<+T>(xs: List<T>) -> Array<T> {
    <(xs, Array::Empty) | push_all
}

func fmt_list<+T: Display>(xs: List<T>) -> String {
    of xs {
        Nil => "",
        Cons(h, Nil) => <h | fmt,
        Cons(h, rest) => (<(<h | fmt, ", ") | add | x => (x, <rest | fmt_list) | add),
    }
}

impl<+T: Display> Display for Array<T> {
    func fmt(self: Array<T>) -> String { <(<self | to_list | fmt_list) | wrap }
}
