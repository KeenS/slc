// `set`: the ordered set. It is `Map` with nothing stored beside the key, so
// the keys come out in order. A key needs `Ord`, and the same total order
// `Map` asks for. `Builder` assembles one the way `map::Builder` does: `put`
// takes the key alone.

use list::List;
use list::List::*;

pub enum Set<+K> {
    Of(map::Map<K, (,)>),
}

pub fn empty<+K>() -> Set<K> {
    Set::Of(map::empty())
}

pub fn length<+K>(s: Set<K>) -> i64 {
    match s {
        Set::Of(m) => <m | map::length,
    }
}

pub fn insert<+K: Ord>(s: Set<K>, key: K) -> Set<K> {
    match s {
        Set::Of(m) => Set::Of(<(m, key, (,)) | map::insert),
    }
}

pub fn remove<+K: Ord>(s: Set<K>, key: K) -> Set<K> {
    match s {
        Set::Of(m) => Set::Of(<(m, key) | map::remove),
    }
}

pub fn contains<+K: Ord>(s: Set<K>, key: K) -> Bool {
    match s {
        Set::Of(m) => <(m, key) | map::contains,
    }
}

fn keys<+K>(xs: List<(K, (,))>) -> List<K> {
    match xs {
        Nil => Nil,
        Cons((k, _), rest) => Cons(k, <rest | keys),
    }
}

pub fn to_list<+K>(s: Set<K>) -> List<K> {
    match s {
        Set::Of(m) => <(<m | map::to_list) | keys,
    }
}

fn of_onto<+K: Ord>(xs: List<K>, s: Set<K>) -> Set<K> {
    match xs {
        Nil => s,
        Cons(k, rest) => <(rest, <(s, k) | insert) | of_onto,
    }
}

pub fn of_list<+K: Ord>(xs: List<K>) -> Set<K> {
    <(xs, empty()) | of_onto
}

fn fmt_keys<+K: Display>(xs: List<K>) -> String {
    match xs {
        Nil => "",
        Cons(k, Nil) => <k | fmt,
        Cons(k, rest) => (<(<k | fmt, ", ") | add | x => (x, <rest | fmt_keys) | add),
    }
}

impl<+K: Display + Ord> Display for Set<K> {
    fn fmt(self: Set<K>) -> String {
        (<("{", <self | to_list | fmt_keys) | add | x => (x, "}") | add)
    }
}

pub menu Builder<+K> {
    put: (K -> Builder<K>),
    finish: Set<K>,
}

fn holding<+K: Ord>(s: Set<K>) -> Builder<K> {
    mu Builder {
        put <= <fn(key: K) { <(<(s, key) | insert) | holding } | put>,
        finish <= <s | finish>,
    }
}

pub fn builder<+K: Ord>() -> Builder<K> {
    <empty() | holding
}

pub fn put<+K: Ord>(b: Builder<K>, key: K) -> Builder<K> {
    <key | b.put
}
