// `map`: a persistent ordered map.
//
// A list finds a value by walking from the head. A map finds one by the
// order of its keys, and an update answers a new map that shares the branch
// it did not walk. The tree is AVL: each node stores its height and leans by
// at most one, so inserting keys in order does not collapse into a list.
//
// Two keys are the same when neither is less than the other. `Ord` has to be
// a total order for that to mean equality: a `NaN` compares that way with
// every float, so it collides with the node the search reaches.
//
// `get` can find nothing, so it offers that outcome to a continuation, the
// way `list::nth` does. Keys and values are positive. `Builder` is the
// negative way to assemble one: `put` answers the function to the next
// builder, and `finish` answers the map that builder holds. The states are
// persistent, so a shared prefix can diverge. `Stream` and `Seq` stay the
// negative sequences.

use list::List::*;

pub enum Map<+K, +V> {
    Empty,
    Node(Map<K, V>, K, V, Map<K, V>, i64),
}

use Map::*;

pub func empty<+K, +V>() -> Map<K, V> {
    Empty
}

pub func length<+K, +V>(m: Map<K, V>) -> i64 {
    of m {
        Empty => 0,
        Node(left, _, _, right, _) => (<(<(1, <left | length) | add, <right | length) | add),
    }
}

func height<+K, +V>(m: Map<K, V>) -> i64 {
    of m {
        Empty => 0,
        Node(_, _, _, _, h) => h,
    }
}

func larger(a: i64, b: i64) -> i64 {
    of (<(a, b) | lt) {
        True => b,
        _ => a,
    }
}

func node<+K, +V>(left: Map<K, V>, key: K, value: V, right: Map<K, V>) -> Map<K, V> {
    let h = <(1, <(<left | height, <right | height) | larger) | add;
    Node(left, key, value, right, h)
}

// The left child is at least two taller. Its lean decides whether one
// rotation restores the bound, or its right spine has to come up first.
// `balance_right` is the mirror.
func balance_left<+K, +V>(left: Map<K, V>, key: K, value: V, right: Map<K, V>) -> Map<K, V> {
    of left {
        Empty => <(left, key, value, right) | node,
        Node(ll, lk, lv, lr, _) => of (<(<ll | height, <lr | height) | ge) {
            True => <(ll, lk, lv, <(lr, key, value, right) | node) | node,
            _ => of lr {
                Empty => <(ll, lk, lv, <(lr, key, value, right) | node) | node,
                Node(lrl, lrk, lrv, lrr, _) => {
                    <(<(ll, lk, lv, lrl) | node, lrk, lrv, <(lrr, key, value, right) | node) | node
                },
            },
        },
    }
}

func balance_right<+K, +V>(left: Map<K, V>, key: K, value: V, right: Map<K, V>) -> Map<K, V> {
    of right {
        Empty => <(left, key, value, right) | node,
        Node(rl, rk, rv, rr, _) => of (<(<rr | height, <rl | height) | ge) {
            True => <(<(left, key, value, rl) | node, rk, rv, rr) | node,
            _ => of rl {
                Empty => <(<(left, key, value, rl) | node, rk, rv, rr) | node,
                Node(rll, rlk, rlv, rlr, _) => {
                    <(<(left, key, value, rll) | node, rlk, rlv, <(rlr, rk, rv, rr) | node) | node
                },
            },
        },
    }
}

func balance<+K, +V>(left: Map<K, V>, key: K, value: V, right: Map<K, V>) -> Map<K, V> {
    let lh = <left | height;
    let rh = <right | height;
    of (<(lh, <(rh, 1) | add) | gt) {
        True => <(left, key, value, right) | balance_left,
        _ => of (<(rh, <(lh, 1) | add) | gt) {
            True => <(left, key, value, right) | balance_right,
            _ => <(left, key, value, right) | node,
        },
    }
}

pub func insert<+K: Ord, +V>(m: Map<K, V>, key: K, value: V) -> Map<K, V> {
    of m {
        Empty => <(Empty, key, value, Empty) | node,
        Node(left, k, v, right, _) => of (<(key, k) | lt) {
            True => <(<(left, key, value) | insert, k, v, right) | balance,
            _ => of (<(k, key) | lt) {
                True => <(left, k, v, <(right, key, value) | insert) | balance,
                _ => <(left, key, value, right) | node,
            },
        },
    }
}

// The least key of a non-empty tree, and the tree with that key removed.
enum Least<+K, +V> {
    Absent,
    Found(K, V, Map<K, V>),
}

use Least::*;

func take_least<+K, +V>(m: Map<K, V>) -> Least<K, V> {
    of m {
        Empty => Absent,
        Node(Empty, k, v, right, _) => Found(k, v, right),
        Node(left, k, v, right, _) => of <left | take_least {
            Absent => Absent,
            Found(mk, mv, new_left) => Found(mk, mv, <(new_left, k, v, right) | balance),
        },
    }
}

// Every key on the left is less than every key on the right: the two sides
// of a key that was deleted.
func join<+K, +V>(left: Map<K, V>, right: Map<K, V>) -> Map<K, V> {
    of left {
        Empty => right,
        _ => of <right | take_least {
            Absent => left,
            Found(k, v, rest) => <(left, k, v, rest) | balance,
        },
    }
}

func delete<+K: Ord, +V>(m: Map<K, V>, key: K) -> Map<K, V> {
    of m {
        Empty => Empty,
        Node(left, k, v, right, _) => of (<(key, k) | lt) {
            True => <(<(left, key) | delete, k, v, right) | balance,
            _ => of (<(k, key) | lt) {
                True => <(left, k, v, <(right, key) | delete) | balance,
                _ => <(left, right) | join,
            },
        },
    }
}

pub func remove<+K: Ord, +V>(m: Map<K, V>, key: K) -> Map<K, V> {
    of (<(m, key) | contains) {
        False => m,
        _ => <(m, key) | delete,
    }
}

pub func contains<+K: Ord, +V>(m: Map<K, V>, key: K) -> Bool {
    of m {
        Empty => False,
        Node(left, k, _, right, _) => of (<(key, k) | lt) {
            True => <(left, key) | contains,
            _ => of (<(k, key) | lt) {
                True => <(right, key) | contains,
                _ => True,
            },
        },
    }
}

pub proc get<+K: Ord, +V, E>(m: Map<K, V>, key: K) | (
    found: (-V / {..E})
    & missing: (-String / {..E})
) / {..E} {
    of m {
        Empty => <"nothing for that key" | missing>,
        Node(left, k, v, right, _) => of (<(key, k) | lt) {
            True => <(left, key) | get | (found & missing)>,
            _ => of (<(k, key) | lt) {
                True => <(right, key) | get | (found & missing)>,
                _ => <v | found>,
            },
        },
    }
}

func to_list_onto<+K, +V>(m: Map<K, V>, tail: list::List<(K, V)>) -> list::List<(K, V)> {
    of m {
        Empty => tail,
        Node(left, k, v, right, _) => {
            <(left, Cons((k, v), <(right, tail) | to_list_onto)) | to_list_onto
        },
    }
}

pub func to_list<+K, +V>(m: Map<K, V>) -> list::List<(K, V)> {
    <(m, Nil) | to_list_onto
}

func of_list_onto<+K: Ord, +V>(pairs: list::List<(K, V)>, m: Map<K, V>) -> Map<K, V> {
    of pairs {
        Nil => m,
        Cons((k, v), rest) => <(rest, <(m, k, v) | insert) | of_list_onto,
    }
}

// Left to right: a later pair with an equal key replaces the earlier one.
pub func of_list<+K: Ord, +V>(pairs: list::List<(K, V)>) -> Map<K, V> {
    <(pairs, Empty) | of_list_onto
}

func fmt_entry<+K: Display, +V: Display>(key: K, value: V) -> String {
    (<(<key | fmt, ": ") | add | x => (x, <value | fmt) | add)
}

func fmt_entries<+K: Display, +V: Display>(xs: list::List<(K, V)>) -> String {
    of xs {
        Nil => "",
        Cons((k, v), Nil) => <(k, v) | fmt_entry,
        Cons((k, v), rest) => {
            (<(<(k, v) | fmt_entry, ", ") | add | x => (x, <rest | fmt_entries) | add)
        },
    }
}

impl<+K: Display + Ord, +V: Display> Display for Map<K, V> {
    func fmt(self: Map<K, V>) -> String {
        (<("{", <self | to_list | fmt_entries) | add | x => (x, "}") | add)
    }
}

// A builder is the map's negative side. Each state closes over one map.
// Asking for `put` gives a function from an entry to the next state; asking
// for `finish` gives the map that state holds.
pub menu Builder<+K, +V> {
    put: ((K, V) -> Builder<K, V>),
    finish: Map<K, V>,
}

func holding<+K: Ord, +V>(m: Map<K, V>) -> Builder<K, V> {
    mu Builder {
        put <= <fn(entry: (K, V)) {
            of entry {
                (key, value) => <(<(m, key, value) | insert) | holding,
            }
        } | put>,
        finish <= <m | finish>,
    }
}

pub func builder<+K: Ord, +V>() -> Builder<K, V> {
    <empty() | holding
}

pub func put<+K: Ord, +V>(b: Builder<K, V>, key: K, value: V) -> Builder<K, V> {
    <(key, value) | b.put
}
