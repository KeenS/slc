// `hashmap`: a persistent hash map.
//
// The path is the hash in base four. Each digit selects a slot of an
// `Array4`, and a bitmap says which of the four slots are occupied. The
// children are packed from the left in slot order. Two keys with the same
// hash share a list. `insert` and `remove` answer a new map. `get` can find
// nothing, so it offers that outcome to a continuation, as `map::get` does.
//
// A key needs `Hash` and `Eq`. Equal keys must hash equal; the checker
// cannot see that.

use list::List;
use list::List::*;

enum Node<+K, +V> {
    Leaf(K, V),
    Knot(List<(K, V)>),
    Branch(u64, array::Array4<Node<K, V>>),
}

enum Hit<+V> {
    Yes(V),
    No,
}

enum Cut<+K, +V> {
    Kept(Node<K, V>),
    Dropped,
}

enum KnotCut<+K, +V> {
    Gone,
    One(K, V),
    Many(List<(K, V)>),
}

enum Packed<+T> {
    None,
    Some(array::Array4<T>),
}

pub enum HashMap<+K, +V> {
    Empty,
    Root(Node<K, V>, i64),
}

fn word(n: u64) -> u64 {
    n
}

fn mask(slot: u64) -> u64 {
    match (<(slot, 0) | eq) {
        True => <1 | word,
        _ => match (<(slot, 1) | eq) {
            True => <2 | word,
            _ => match (<(slot, 2) | eq) {
                True => <4 | word,
                _ => <8 | word,
            },
        },
    }
}

fn bit_set(bitmap: u64, slot: u64) -> Bool {
    let m = <slot | mask;
    let shifted = <(bitmap, m) | div;
    let bit = <(shifted, 2) | rem;
    <(bit, 1) | eq
}

fn one_if(bitmap: u64, slot: u64) -> u64 {
    match (<(bitmap, slot) | bit_set) {
        True => <1 | word,
        _ => <0 | word,
    }
}

// How many occupied slots sit strictly before `slot`. That is the index in
// the packed `Array4`.
fn below(bitmap: u64, slot: u64) -> u64 {
    match (<(slot, 0) | eq) {
        True => <0 | word,
        _ => match (<(slot, 1) | eq) {
            True => <(bitmap, 0) | one_if,
            _ => match (<(slot, 2) | eq) {
                True => {
                    let a = <(bitmap, 0) | one_if;
                    let b = <(bitmap, 1) | one_if;
                    <(a, b) | add
                },
                _ => {
                    let a = <(bitmap, 0) | one_if;
                    let b = <(bitmap, 1) | one_if;
                    let c = <(bitmap, 2) | one_if;
                    <(<(a, b) | add, c) | add
                },
            },
        },
    }
}

fn child<+T>(kids: array::Array4<T>, i: u64) -> T {
    match kids {
        array::Array4::One(a0) => a0,
        array::Array4::Two(a0, a1) => match (<(i, 0) | eq) {
            True => a0,
            _ => a1,
        },
        array::Array4::Three(a0, a1, a2) => match (<(i, 0) | eq) {
            True => a0,
            _ => match (<(i, 1) | eq) {
                True => a1,
                _ => a2,
            },
        },
        array::Array4::Four(a0, a1, a2, a3) => match (<(i, 0) | eq) {
            True => a0,
            _ => match (<(i, 1) | eq) {
                True => a1,
                _ => match (<(i, 2) | eq) {
                    True => a2,
                    _ => a3,
                },
            },
        },
    }
}

fn swap_child<+T>(kids: array::Array4<T>, i: u64, value: T) -> array::Array4<T> {
    match kids {
        array::Array4::One(_) => array::Array4::One(value),
        array::Array4::Two(a0, a1) => match (<(i, 0) | eq) {
            True => array::Array4::Two(value, a1),
            _ => array::Array4::Two(a0, value),
        },
        array::Array4::Three(a0, a1, a2) => match (<(i, 0) | eq) {
            True => array::Array4::Three(value, a1, a2),
            _ => match (<(i, 1) | eq) {
                True => array::Array4::Three(a0, value, a2),
                _ => array::Array4::Three(a0, a1, value),
            },
        },
        array::Array4::Four(a0, a1, a2, a3) => match (<(i, 0) | eq) {
            True => array::Array4::Four(value, a1, a2, a3),
            _ => match (<(i, 1) | eq) {
                True => array::Array4::Four(a0, value, a2, a3),
                _ => match (<(i, 2) | eq) {
                    True => array::Array4::Four(a0, a1, value, a3),
                    _ => array::Array4::Four(a0, a1, a2, value),
                },
            },
        },
    }
}

fn insert_child<+T>(kids: array::Array4<T>, i: u64, value: T) -> array::Array4<T> {
    match kids {
        array::Array4::One(a0) => match (<(i, 0) | eq) {
            True => array::Array4::Two(value, a0),
            _ => array::Array4::Two(a0, value),
        },
        array::Array4::Two(a0, a1) => match (<(i, 0) | eq) {
            True => array::Array4::Three(value, a0, a1),
            _ => match (<(i, 1) | eq) {
                True => array::Array4::Three(a0, value, a1),
                _ => array::Array4::Three(a0, a1, value),
            },
        },
        array::Array4::Three(a0, a1, a2) => match (<(i, 0) | eq) {
            True => array::Array4::Four(value, a0, a1, a2),
            _ => match (<(i, 1) | eq) {
                True => array::Array4::Four(a0, value, a1, a2),
                _ => match (<(i, 2) | eq) {
                    True => array::Array4::Four(a0, a1, value, a2),
                    _ => array::Array4::Four(a0, a1, a2, value),
                },
            },
        },
        array::Array4::Four(a0, a1, a2, a3) => array::Array4::Four(a0, a1, a2, a3),
    }
}

fn remove_child<+T>(kids: array::Array4<T>, i: u64) -> Packed<T> {
    match kids {
        array::Array4::One(_) => Packed::None,
        array::Array4::Two(a0, a1) => match (<(i, 0) | eq) {
            True => Packed::Some(array::Array4::One(a1)),
            _ => Packed::Some(array::Array4::One(a0)),
        },
        array::Array4::Three(a0, a1, a2) => match (<(i, 0) | eq) {
            True => Packed::Some(array::Array4::Two(a1, a2)),
            _ => match (<(i, 1) | eq) {
                True => Packed::Some(array::Array4::Two(a0, a2)),
                _ => Packed::Some(array::Array4::Two(a0, a1)),
            },
        },
        array::Array4::Four(a0, a1, a2, a3) => match (<(i, 0) | eq) {
            True => Packed::Some(array::Array4::Three(a1, a2, a3)),
            _ => match (<(i, 1) | eq) {
                True => Packed::Some(array::Array4::Three(a0, a2, a3)),
                _ => match (<(i, 2) | eq) {
                    True => Packed::Some(array::Array4::Three(a0, a1, a3)),
                    _ => Packed::Some(array::Array4::Three(a0, a1, a2)),
                },
            },
        },
    }
}

fn slot_of(hash: u64, shift: u64) -> u64 {
    <(<(hash, shift) | div, 4) | rem
}

// Both hashes still have bits, or they share every remaining digit and the
// keys are kept together.
fn unite<+K, +V>(k1: K, v1: V, h1: u64, k2: K, v2: V, h2: u64, shift: u64) -> Node<K, V> {
    let r1 = <(h1, shift) | div;
    let r2 = <(h2, shift) | div;
    match (<(r1, 0) | eq) {
        True => match (<(r2, 0) | eq) {
            True => Node::Knot(Cons((k1, v1), Cons((k2, v2), Nil))),
            _ => <(k1, v1, h1, k2, v2, h2, shift) | split,
        },
        _ => <(k1, v1, h1, k2, v2, h2, shift) | split,
    }
}

fn split<+K, +V>(k1: K, v1: V, h1: u64, k2: K, v2: V, h2: u64, shift: u64) -> Node<K, V> {
    let s1 = <(h1, shift) | slot_of;
    let s2 = <(h2, shift) | slot_of;
    match (<(s1, s2) | eq) {
        True => {
            let deeper = <(k1, v1, h1, k2, v2, h2, <(shift, 4) | mul) | unite;
            Node::Branch(<s1 | mask, array::Array4::One(deeper))
        },
        _ => match (<(s1, s2) | lt) {
            True => {
                Node::Branch(
                    <(<s1 | mask, <s2 | mask) | add,
                    array::Array4::Two(Node::Leaf(k1, v1), Node::Leaf(k2, v2)),
                )
            },
            _ => {
                Node::Branch(
                    <(<s1 | mask, <s2 | mask) | add,
                    array::Array4::Two(Node::Leaf(k2, v2), Node::Leaf(k1, v1)),
                )
            },
        },
    }
}

fn knot_lookup<+K: Eq, +V>(pairs: List<(K, V)>, key: K) -> Hit<V> {
    match pairs {
        Nil => Hit::No,
        Cons((k, v), rest) => match (<(key, k) | eq) {
            True => Hit::Yes(v),
            _ => <(rest, key) | knot_lookup,
        },
    }
}

fn knot_place<+K: Eq, +V>(pairs: List<(K, V)>, key: K, value: V) -> (List<(K, V)>, i64) {
    match pairs {
        Nil => (Cons((key, value), Nil), 1),
        Cons((k, v), rest) => match (<(key, k) | eq) {
            True => (Cons((key, value), rest), 0),
            _ => {
                let placed = <(rest, key, value) | knot_place;
                (Cons((k, v), placed.0), placed.1)
            },
        },
    }
}

fn knot_rest<+K, +V>(pairs: List<(K, V)>) -> KnotCut<K, V> {
    match pairs {
        Nil => KnotCut::Gone,
        Cons((k, v), Nil) => KnotCut::One(k, v),
        many => KnotCut::Many(many),
    }
}

fn knot_cut<+K: Eq, +V>(pairs: List<(K, V)>, key: K) -> KnotCut<K, V> {
    match pairs {
        Nil => KnotCut::Gone,
        Cons((k, v), rest) => match (<(key, k) | eq) {
            True => <rest | knot_rest,
            _ => match <(rest, key) | knot_cut {
                KnotCut::Gone => KnotCut::One(k, v),
                KnotCut::One(k2, v2) => KnotCut::Many(Cons((k, v), Cons((k2, v2), Nil))),
                KnotCut::Many(xs) => KnotCut::Many(Cons((k, v), xs)),
            },
        },
    }
}

fn lookup<+K: Hash + Eq, +V>(node: Node<K, V>, key: K, full: u64, shift: u64) -> Hit<V> {
    match node {
        Node::Leaf(k, v) => match (<(key, k) | eq) {
            True => Hit::Yes(v),
            _ => Hit::No,
        },
        Node::Knot(pairs) => <(pairs, key) | knot_lookup,
        Node::Branch(bitmap, children) => {
            let slot = <(full, shift) | slot_of;
            match (<(bitmap, slot) | bit_set) {
                False => Hit::No,
                _ => {
                    let index = <(bitmap, slot) | below;
                    let found = <(children, index) | child;
                    <(found, key, full, <(shift, 4) | mul) | lookup
                },
            }
        },
    }
}

fn place<+K: Hash + Eq, +V>(
    node: Node<K, V>,
    key: K,
    value: V,
    full: u64,
    shift: u64,
) -> (Node<K, V>, i64) {
    match node {
        Node::Leaf(k, v) => match (<(key, k) | eq) {
            True => (Node::Leaf(key, value), 0),
            _ => {
                let old = <k | hash;
                match (<(old, full) | eq) {
                    True => (Node::Knot(Cons((key, value), Cons((k, v), Nil))), 1),
                    _ => (<(k, v, old, key, value, full, shift) | unite, 1),
                }
            },
        },
        Node::Knot(pairs) => {
            let placed = <(pairs, key, value) | knot_place;
            (Node::Knot(placed.0), placed.1)
        },
        Node::Branch(bitmap, children) => {
            let slot = <(full, shift) | slot_of;
            let next = <(shift, 4) | mul;
            match (<(bitmap, slot) | bit_set) {
                False => {
                    let index = <(bitmap, slot) | below;
                    let kids = <(children, index, Node::Leaf(key, value)) | insert_child;
                    let bits = <(bitmap, <slot | mask) | add;
                    (Node::Branch(bits, kids), 1)
                },
                _ => {
                    let index = <(bitmap, slot) | below;
                    let found = <(children, index) | child;
                    let placed = <(found, key, value, full, next) | place;
                    let kids = <(children, index, placed.0) | swap_child;
                    (Node::Branch(bitmap, kids), placed.1)
                },
            }
        },
    }
}

fn cut<+K: Hash + Eq, +V>(node: Node<K, V>, key: K, full: u64, shift: u64) -> Cut<K, V> {
    match node {
        Node::Leaf(k, _) => match (<(key, k) | eq) {
            True => Cut::Dropped,
            _ => Cut::Kept(node),
        },
        Node::Knot(pairs) => match <(pairs, key) | knot_cut {
            KnotCut::Gone => Cut::Dropped,
            KnotCut::One(k, v) => Cut::Kept(Node::Leaf(k, v)),
            KnotCut::Many(xs) => Cut::Kept(Node::Knot(xs)),
        },
        Node::Branch(bitmap, children) => {
            let slot = <(full, shift) | slot_of;
            match (<(bitmap, slot) | bit_set) {
                False => Cut::Kept(node),
                _ => {
                    let index = <(bitmap, slot) | below;
                    let found = <(children, index) | child;
                    match <(found, key, full, <(shift, 4) | mul) | cut {
                        Cut::Kept(child) => {
                            Cut::Kept(Node::Branch(bitmap, <(children, index, child) | swap_child))
                        },
                        Cut::Dropped => match <(children, index) | remove_child {
                            Packed::None => Cut::Dropped,
                            Packed::Some(kids) => {
                                let bits = <(bitmap, <slot | mask) | sub;
                                Cut::Kept(Node::Branch(bits, kids))
                            },
                        },
                    }
                },
            }
        },
    }
}

pub fn empty<+K, +V>() -> HashMap<K, V> {
    HashMap::Empty
}

pub fn length<+K, +V>(m: HashMap<K, V>) -> i64 {
    match m {
        HashMap::Empty => 0,
        HashMap::Root(_, n) => n,
    }
}

pub fn insert<+K: Hash + Eq, +V>(m: HashMap<K, V>, key: K, value: V) -> HashMap<K, V> {
    let hash = <key | hash;
    match m {
        HashMap::Empty => HashMap::Root(Node::Leaf(key, value), 1),
        HashMap::Root(node, n) => {
            let placed = <(node, key, value, hash, 1) | place;
            HashMap::Root(placed.0, <(n, placed.1) | add)
        },
    }
}

pub fn contains<+K: Hash + Eq, +V>(m: HashMap<K, V>, key: K) -> Bool {
    match m {
        HashMap::Empty => False,
        HashMap::Root(node, _) => match <(node, key, <key | hash, 1) | lookup {
            Hit::Yes(_) => True,
            Hit::No => False,
        },
    }
}

fn delete<+K: Hash + Eq, +V>(m: HashMap<K, V>, key: K) -> HashMap<K, V> {
    match m {
        HashMap::Empty => HashMap::Empty,
        HashMap::Root(node, n) => match <(node, key, <key | hash, 1) | cut {
            Cut::Dropped => HashMap::Empty,
            Cut::Kept(node) => HashMap::Root(node, <(n, 1) | sub),
        },
    }
}

pub fn remove<+K: Hash + Eq, +V>(m: HashMap<K, V>, key: K) -> HashMap<K, V> {
    match (<(m, key) | contains) {
        False => m,
        _ => <(m, key) | delete,
    }
}

pub command get<+K: Hash + Eq, +V, E>(m: HashMap<K, V>, key: K) | (
    found: (-V / {..E})
    & missing: (-String / {..E})
) / {..E} {
    match m {
        HashMap::Empty => <"nothing for that key" | missing>,
        HashMap::Root(node, _) => match <(node, key, <key | hash, 1) | lookup {
            Hit::Yes(v) => <v | found>,
            Hit::No => <"nothing for that key" | missing>,
        },
    }
}

fn append_pairs<+K, +V>(xs: List<(K, V)>, tail: List<(K, V)>) -> List<(K, V)> {
    match xs {
        Nil => tail,
        Cons(h, rest) => Cons(h, <(rest, tail) | append_pairs),
    }
}

fn node_onto<+K, +V>(node: Node<K, V>, tail: List<(K, V)>) -> List<(K, V)> {
    match node {
        Node::Leaf(k, v) => Cons((k, v), tail),
        Node::Knot(pairs) => <(pairs, tail) | append_pairs,
        Node::Branch(_, children) => match children {
            array::Array4::One(a) => <(a, tail) | node_onto,
            array::Array4::Two(a, b) => <(a, <(b, tail) | node_onto) | node_onto,
            array::Array4::Three(a, b, c) => <(a, <(b, <(c, tail) | node_onto) | node_onto)
                | node_onto,
            array::Array4::Four(a, b, c, d) => {
                <(a, <(b, <(c, <(d, tail) | node_onto) | node_onto) | node_onto) | node_onto
            },
        },
    }
}

pub fn to_list<+K, +V>(m: HashMap<K, V>) -> List<(K, V)> {
    match m {
        HashMap::Empty => Nil,
        HashMap::Root(node, _) => <(node, Nil) | node_onto,
    }
}

fn of_onto<+K: Hash + Eq, +V>(pairs: List<(K, V)>, m: HashMap<K, V>) -> HashMap<K, V> {
    match pairs {
        Nil => m,
        Cons((k, v), rest) => <(rest, <(m, k, v) | insert) | of_onto,
    }
}

// Left to right: a later pair with an equal key replaces the earlier one.
pub fn of_list<+K: Hash + Eq, +V>(pairs: List<(K, V)>) -> HashMap<K, V> {
    <(pairs, HashMap::Empty) | of_onto
}

fn fmt_entry<+K: Display, +V: Display>(key: K, value: V) -> String {
    (<(<key | fmt, ": ") | add | x => (x, <value | fmt) | add)
}

fn fmt_entries<+K: Display, +V: Display>(xs: List<(K, V)>) -> String {
    match xs {
        Nil => "",
        Cons((k, v), Nil) => <(k, v) | fmt_entry,
        Cons((k, v), rest) => {
            (<(<(k, v) | fmt_entry, ", ") | add | x => (x, <rest | fmt_entries) | add)
        },
    }
}

impl<+K: Display, +V: Display> Display for HashMap<K, V> {
    fn fmt(self: HashMap<K, V>) -> String {
        (<("{", <self | to_list | fmt_entries) | add | x => (x, "}") | add)
    }
}

// Same menu as `map::Builder`. Membership is `Hash` and `Eq`, so `put`
// follows the trie rather than the key order.
pub menu Builder<+K, +V> {
    put: ((K, V) -> Builder<K, V>),
    finish: HashMap<K, V>,
}

fn holding<+K: Hash + Eq, +V>(m: HashMap<K, V>) -> Builder<K, V> {
    mu Builder {
        put <= <fn(entry: (K, V)) {
            match entry {
                (key, value) => <(<(m, key, value) | insert) | holding,
            }
        } | put>,
        finish <= <m | finish>,
    }
}

pub fn builder<+K: Hash + Eq, +V>() -> Builder<K, V> {
    <empty() | holding
}

pub fn put<+K: Hash + Eq, +V>(b: Builder<K, V>, key: K, value: V) -> Builder<K, V> {
    <(key, value) | b.put
}
