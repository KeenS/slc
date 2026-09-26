// Builders are the negative side of the maps and sets.
//
// Each is a menu. `put` answers a function to the next builder, and `finish`
// answers the collection that builder holds. The states are persistent, so
// two builders share a prefix and then diverge, and finishing one leaves it
// ready to be extended. `map` and `set` order their keys. `hashmap` and
// `hashset` keep hash-slot order.

proc main | (exit: i32) / {IO} {
    let start = map::builder();
    // Demanded on the menu, then through `put`.
    let left = <("m", 1) | start.put;
    let left = <(left, "a", 2) | map::put;
    let right = <(start, "z", 9) | map::put;
    <left.finish | println;
    <right.finish | println;
    <start.finish | println;
    let more = <(left, "b", 4) | map::put;
    <left.finish | println;
    <more.finish | println;

    let nums = <(set::builder(), 2) | set::put;
    let nums = <(nums, 1) | set::put;
    let nums = <(nums, 2) | set::put;
    <nums.finish | println;

    let hashed = <(hashmap::builder(), "m", 1) | hashmap::put;
    let hashed = <(hashed, "a", 2) | hashmap::put;
    <hashed.finish | println;

    let keys = <(hashset::builder(), "b") | hashset::put;
    let keys = <(keys, "a") | hashset::put;
    let keys = <(keys, "b") | hashset::put;
    <keys.finish | println;
    <0 | exit>
}
