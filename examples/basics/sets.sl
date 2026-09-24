// HashMap, HashSet, and the ordered Set.
//
// `HashMap` walks the hash in base four. `HashSet` is that map with nothing
// beside the key. `Set` is the ordered set: `Map` with nothing beside the
// key, so the keys come out in order. Two keys with one hash share a list.

use list::List::*;

enum Id {
    A,
    B,
}

impl Eq for Id {
    fn eq(self: Id, other: Id) -> Bool {
        match self {
            A => match other {
                A => True,
                _ => False,
            },
            B => match other {
                B => True,
                _ => False,
            },
        }
    }
    fn ne(self: Id, other: Id) -> Bool {
        match self {
            A => match other {
                A => False,
                _ => True,
            },
            B => match other {
                B => False,
                _ => True,
            },
        }
    }
}

impl Hash for Id {
    fn hash(self: Id) -> u64 {
        0
    }
}

impl Display for Id {
    fn fmt(self: Id) -> String {
        match self {
            A => "A",
            B => "B",
        }
    }
}

command main | (exit: i32) / {IO} {
    let base = <(hashmap::empty(), "m", 1) | hashmap::insert;
    let grown = <(<(base, "a", 2) | hashmap::insert, "m", 9) | hashmap::insert;
    <base | fmt | println; // {m: 1}
    <grown | fmt | println; // {m: 9, a: 2}
    <grown | hashmap::length | println; // 2
    <mu i64 {
        out <= <(grown, "m") | hashmap::get | (out & select String { _ => <0 | out> })>,
    } | println; // 9
    <mu i64 {
        out <= <(grown, "z") | hashmap::get | (out & select String { _ => <-1 | out> })>,
    } | println; // -1
    <(grown, "a") | hashmap::contains | println; // true
    <(<(grown, "nope") | hashmap::remove) | hashmap::length | println; // 2
    <(<(grown, "a") | hashmap::remove) | fmt | println; // {m: 9}
    <Cons(("c", 1), Cons(("a", 9), Cons(("c", 4), Nil)))
        | hashmap::of_list
        | fmt
        | println; // {a: 9, c: 4}

    // A and B hash equal, so they share a knot. Removing one leaves the other.
    let both = <(<(hashmap::empty(), A, 1) | hashmap::insert, B, 2) | hashmap::insert;
    <both | hashmap::length | println; // 2
    <(both, A) | hashmap::contains | println; // true
    <(both, B) | hashmap::contains | println; // true
    let left = <(both, A) | hashmap::remove;
    <(left, A) | hashmap::contains | println; // false
    <(left, B) | hashmap::contains | println; // true
    <left | fmt | println; // {B: 2}

    let names = <(<(hashset::empty(), "b") | hashset::insert, "a") | hashset::insert;
    let names = <(names, "b") | hashset::insert;
    <names | hashset::length | println; // 2
    <names | fmt | println; // {a, b}
    <(<(names, "b") | hashset::remove) | fmt | println; // {a}

    let keys = <(<(<(set::empty(), 2) | set::insert, 1) | set::insert, 3) | set::insert;
    <keys | fmt | println; // {1, 2, 3}
    <keys | set::to_list | fmt | println; // [1, 2, 3]
    <(<(keys, 2) | set::remove) | fmt | println; // {1, 3}
    <(<(keys, 9) | set::remove) | set::length | println; // 3

    <0 | exit>
}
