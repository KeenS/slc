// Ordered maps: a persistent AVL tree in the library.
//
// Nothing about `Map` is built in. Keys use the prelude's `Ord`, `String`
// and `Bool` included, and an update answers a new map. `get` offers a found
// value or a missing message, as `list::nth` does. Inserting 1..7 in order
// stands at height 3: the tree leaned, and was restored.

use list::List;
use list::List::*;
use map::Map;

fn height(m: Map<i64, i64>) -> i64 {
    match m {
        Map::Empty => 0,
        Map::Node(_, _, _, _, h) => h,
    }
}

fn range(n: i64, limit: i64) -> List<(i64, i64)> {
    match (<(n, limit) | gt) {
        True => Nil,
        _ => Cons((n, n), <(<(n, 1) | add, limit) | range),
    }
}

command main | (exit: i32) / {IO} {
    // The order the tree is built on.
    <("a", "b") | lt | println; // true
    <("b", "a") | lt | println; // false
    <("app", "apple") | lt | println; // true
    <(False, True) | lt | println; // true
    <('a', 'c') | lt | println; // true

    // `insert` leaves the map it was given.
    let base = <(map::empty(), "m", 1) | map::insert;
    let grown = <(<(base, "a", 2) | map::insert, "z", 3) | map::insert;
    <base | fmt | println; // {m: 1}
    <grown | fmt | println; // {a: 2, m: 1, z: 3}

    <mu i64 {
        out <= <(grown, "m") | map::get | (out & select String { message => <0 | out> })>,
    } | println; // 1
    <mu String {
        out <= <(grown, "no") | map::get | (select i64 { n => <n | int_to_str | out> } & out)>,
    } | println; // nothing for that key

    let replaced = <(grown, "m", 9) | map::insert;
    <replaced | fmt | println; // {a: 2, m: 9, z: 3}
    <grown | fmt | println; // {a: 2, m: 1, z: 3}

    // A later equal key wins. A missing `remove` is the map itself.
    let words = <Cons(("b", 1), Cons(("a", 2), Cons(("b", 3), Nil))) | map::of_list;
    <words | fmt | println; // {a: 2, b: 3}
    <(words, "b") | map::contains | println; // true
    <(words, "c") | map::contains | println; // false
    <words | map::length | println; // 2
    <(<(words, "c") | map::remove) | fmt | println; // {a: 2, b: 3}
    <(<(words, "a") | map::remove) | fmt | println; // {b: 3}

    // Sorted inserts stay balanced, and deletion keeps the order.
    let nums = <(1, 7) | range | map::of_list;
    <nums | map::length | println; // 7
    <nums | height | println; // 3
    <nums
        | map::to_list
        | fmt
        | println; // [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]
    let without = <(nums, 4) | map::remove;
    <without | height | println; // 3
    <without | map::to_list | fmt | println; // [(1, 1), (2, 2), (3, 3), (5, 5), (6, 6), (7, 7)]
    let dropped = <(without, 6) | map::remove;
    <dropped | height | println; // 3
    <dropped | map::to_list | fmt | println; // [(1, 1), (2, 2), (3, 3), (5, 5), (7, 7)]

    // Both leans: the keys arrive so that restoring the height rotates either way.
    let zigzag = <Cons(
        (1, 1),
        Cons((3, 3), Cons((2, 2), Cons((5, 5), Cons((4, 4), Cons((7, 7), Cons((6, 6), Nil)))))),
    )
        | map::of_list;
    <zigzag | height | println; // 3
    <zigzag
        | map::to_list
        | fmt
        | println; // [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]
    let mirror = <Cons(
        (7, 7),
        Cons((5, 5), Cons((6, 6), Cons((3, 3), Cons((4, 4), Cons((1, 1), Cons((2, 2), Nil)))))),
    )
        | map::of_list;
    <mirror | height | println; // 3
    <mirror
        | map::to_list
        | fmt
        | println; // [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]

    let gone = <(<(map::empty(), 1, 1) | map::insert, 1) | map::remove;
    <gone | fmt | println; // {}
    <gone | map::length | println; // 0

    <0 | exit>
}
