// `hashset`: a persistent set of keys, kept in a `HashMap` with nothing
// beside the key. Membership uses `Hash` and `Eq`, as the map does.
// `Builder` assembles one the way `hashmap::Builder` does, and `put` takes
// the key alone.

use list::List;
use list::List::*;

pub enum HashSet<+K> {
    Of(hashmap::HashMap<K, (,)>),
}

pub fn empty<+K>() -> HashSet<K> {
    HashSet::Of(hashmap::empty())
}

pub fn length<+K>(s: HashSet<K>) -> i64 {
    match s {
        HashSet::Of(m) => <m | hashmap::length,
    }
}

pub fn insert<+K: Hash + Eq>(s: HashSet<K>, key: K) -> HashSet<K> {
    match s {
        HashSet::Of(m) => HashSet::Of(<(m, key, (,)) | hashmap::insert),
    }
}

pub fn remove<+K: Hash + Eq>(s: HashSet<K>, key: K) -> HashSet<K> {
    match s {
        HashSet::Of(m) => HashSet::Of(<(m, key) | hashmap::remove),
    }
}

pub fn contains<+K: Hash + Eq>(s: HashSet<K>, key: K) -> Bool {
    match s {
        HashSet::Of(m) => <(m, key) | hashmap::contains,
    }
}

fn keys<+K>(xs: List<(K, (,))>) -> List<K> {
    match xs {
        Nil => Nil,
        Cons((k, _), rest) => Cons(k, <rest | keys),
    }
}

pub fn to_list<+K>(s: HashSet<K>) -> List<K> {
    match s {
        HashSet::Of(m) => <(<m | hashmap::to_list) | keys,
    }
}

fn of_onto<+K: Hash + Eq>(xs: List<K>, s: HashSet<K>) -> HashSet<K> {
    match xs {
        Nil => s,
        Cons(k, rest) => <(rest, <(s, k) | insert) | of_onto,
    }
}

pub fn of_list<+K: Hash + Eq>(xs: List<K>) -> HashSet<K> {
    <(xs, empty()) | of_onto
}

fn fmt_keys<+K: Display>(xs: List<K>) -> String {
    match xs {
        Nil => "",
        Cons(k, Nil) => <k | fmt,
        Cons(k, rest) => (<(<k | fmt, ", ") | add | x => (x, <rest | fmt_keys) | add),
    }
}

impl<+K: Display + Hash + Eq> Display for HashSet<K> {
    fn fmt(self: HashSet<K>) -> String {
        (<("{", <self | to_list | fmt_keys) | add | x => (x, "}") | add)
    }
}

pub menu Builder<+K> {
    put: (K -> Builder<K>),
    finish: HashSet<K>,
}

fn holding<+K: Hash + Eq>(s: HashSet<K>) -> Builder<K> {
    mu Builder {
        put <= <fn(key: K) { <(<(s, key) | insert) | holding } | put>,
        finish <= <s | finish>,
    }
}

pub fn builder<+K: Hash + Eq>() -> Builder<K> {
    <empty() | holding
}

pub fn put<+K: Hash + Eq>(b: Builder<K>, key: K) -> Builder<K> {
    <key | b.put
}
