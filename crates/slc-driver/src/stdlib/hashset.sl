// `hashset`: a persistent set of keys, kept in a `HashMap` with nothing
// beside the key. Membership uses `Hash` and `Eq`, as the map does.
// `Builder` assembles one the way `hashmap::Builder` does, and `put` takes
// the key alone.

cite list::List;
cite list::List::*;

pub enum HashSet<+K> {
    Of(hashmap::HashMap<K, (,)>),
}

pub func empty<+K>() -> HashSet<K> {
    HashSet::Of(hashmap::empty())
}

pub func length<+K>(s: HashSet<K>) -> i64 {
    of s {
        HashSet::Of(m) => <m | hashmap::length,
    }
}

pub func insert<+K: Hash + Eq>(s: HashSet<K>, key: K) -> HashSet<K> {
    of s {
        HashSet::Of(m) => HashSet::Of(<(m, key, (,)) | hashmap::insert),
    }
}

pub func remove<+K: Hash + Eq>(s: HashSet<K>, key: K) -> HashSet<K> {
    of s {
        HashSet::Of(m) => HashSet::Of(<(m, key) | hashmap::remove),
    }
}

pub func contains<+K: Hash + Eq>(s: HashSet<K>, key: K) -> Bool {
    of s {
        HashSet::Of(m) => <(m, key) | hashmap::contains,
    }
}

func keys<+K>(xs: List<(K, (,))>) -> List<K> {
    of xs {
        Nil => Nil,
        Cons((k, _), rest) => Cons(k, <rest | keys),
    }
}

pub func to_list<+K>(s: HashSet<K>) -> List<K> {
    of s {
        HashSet::Of(m) => <(<m | hashmap::to_list) | keys,
    }
}

func of_onto<+K: Hash + Eq>(xs: List<K>, s: HashSet<K>) -> HashSet<K> {
    of xs {
        Nil => s,
        Cons(k, rest) => <(rest, <(s, k) | insert) | of_onto,
    }
}

pub func of_list<+K: Hash + Eq>(xs: List<K>) -> HashSet<K> {
    <(xs, empty()) | of_onto
}

func fmt_keys<+K: Display>(xs: List<K>) -> String {
    of xs {
        Nil => "",
        Cons(k, Nil) => <k | fmt,
        Cons(k, rest) => (<(<k | fmt, ", ") | add | x => (x, <rest | fmt_keys) | add),
    }
}

impl<+K: Display + Hash + Eq> Display for HashSet<K> {
    func fmt(self: HashSet<K>) -> String {
        (<("{", <self | to_list | fmt_keys) | add | x => (x, "}") | add)
    }
}

pub menu Builder<+K> {
    put: (K -> Builder<K>),
    finish: HashSet<K>,
}

func holding<+K: Hash + Eq>(s: HashSet<K>) -> Builder<K> {
    mu Builder {
        put <= <fn(key: K) { <(<(s, key) | insert) | holding } | put>,
        finish <= <s | finish>,
    }
}

pub func builder<+K: Hash + Eq>() -> Builder<K> {
    <empty() | holding
}

pub func put<+K: Hash + Eq>(b: Builder<K>, key: K) -> Builder<K> {
    <key | b.put
}
