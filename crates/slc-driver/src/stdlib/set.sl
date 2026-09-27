// `set`: the ordered set. It is `Map` with nothing stored beside the key, so
// the keys come out in order. A key needs `Ord`, and the same total order
// `Map` asks for. `Builder` assembles one the way `map::Builder` does: `put`
// takes the key alone.

cite list::List;
cite list::List::*;

pub enum Set<+K> {
    Of(map::Map<K, (,)>),
}

pub func empty<+K>() -> Set<K> {
    Set::Of(map::empty())
}

pub func length<+K>(s: Set<K>) -> i64 {
    of s {
        Set::Of(m) => <m | map::length,
    }
}

pub func insert<+K: Ord>(s: Set<K>, key: K) -> Set<K> {
    of s {
        Set::Of(m) => Set::Of(<(m, key, (,)) | map::insert),
    }
}

pub func remove<+K: Ord>(s: Set<K>, key: K) -> Set<K> {
    of s {
        Set::Of(m) => Set::Of(<(m, key) | map::remove),
    }
}

pub func contains<+K: Ord>(s: Set<K>, key: K) -> Bool {
    of s {
        Set::Of(m) => <(m, key) | map::contains,
    }
}

func keys<+K>(xs: List<(K, (,))>) -> List<K> {
    of xs {
        Nil => Nil,
        Cons((k, _), rest) => Cons(k, <rest | keys),
    }
}

pub func to_list<+K>(s: Set<K>) -> List<K> {
    of s {
        Set::Of(m) => <(<m | map::to_list) | keys,
    }
}

func of_onto<+K: Ord>(xs: List<K>, s: Set<K>) -> Set<K> {
    of xs {
        Nil => s,
        Cons(k, rest) => <(rest, <(s, k) | insert) | of_onto,
    }
}

pub func of_list<+K: Ord>(xs: List<K>) -> Set<K> {
    <(xs, empty()) | of_onto
}

func fmt_keys<+K: Display>(xs: List<K>) -> String {
    of xs {
        Nil => "",
        Cons(k, Nil) => <k | fmt,
        Cons(k, rest) => (<(<k | fmt, ", ") | add | x => (x, <rest | fmt_keys) | add),
    }
}

impl<+K: Display + Ord> Display for Set<K> {
    func fmt(self: Set<K>) -> String {
        (<("{", <self | to_list | fmt_keys) | add | x => (x, "}") | add)
    }
}

pub menu Builder<+K> {
    put(key: K): Builder<K>,
    finish: Set<K>,
}

func holding<+K: Ord>(s: Set<K>) -> Builder<K> {
    mu Builder {
        put(key): out <= <(<(s, key) | insert) | holding | out>,
        finish <= <s | finish>,
    }
}

pub func builder<+K: Ord>() -> Builder<K> {
    <empty() | holding
}

pub func put<+K: Ord>(b: Builder<K>, key: K) -> Builder<K> {
    <key | b.put
}
