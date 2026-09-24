// An immutable array: a 4-way trie of `Array4` nodes.
//
// `Array4` is one branch. `Array` is any length. `push` and `update` answer
// a new array. Indices 4, 16, and 19 sit across the boundaries where a new
// branch is grown.

use list::List;
use list::List::*;

fn count(n: i64, limit: i64) -> List<i64> {
    match (<(n, limit) | gt) {
        True => Nil,
        _ => Cons(n, <(<(n, 1) | add, limit) | count),
    }
}

command main | (exit: i32) / {IO} {
    let pair = array::Array4::Two(1, 2);
    <pair | array::slots | println; // 2
    <pair | fmt | println; // [1, 2]
    <mu i64 {
        out <= <(pair, 1) | array::slot_get | (out & select String { _ => <0 | out> })>,
    } | println; // 2
    <mu i64 {
        out <= <(pair, 3) | array::slot_get | (out & select String { _ => <-1 | out> })>,
    } | println; // -1

    let quad = array::Array4::Four("a", "b", "c", "d");
    <quad | array::slots | println; // 4
    <quad | fmt | println; // [a, b, c, d]
    <mu String {
        out <= <(quad, 0) | array::slot_get | (out & select String { _ => <"?" | out> })>,
    } | println; // a
    <mu String {
        out <= <(quad, 3) | array::slot_get | (out & select String { _ => <"?" | out> })>,
    } | println; // d
    let edited = mu array::Array4<String> {
        out <= <(quad, 2, "z") | array::slot_update | (out & select String { _ => <quad | out> })>,
    };
    <edited | fmt | println; // [a, b, z, d]
    <quad | fmt | println; // [a, b, c, d]
    <mu String {
        out <= <(quad, 4, "z") | array::slot_update | (
            fn(node: array::Array4<String>) { <node | fmt | out> }
            & out
        )>,
    } | println; // nothing at that index

    let none: array::Array<i64> = array::empty();
    <none | array::length | println; // 0
    <none | fmt | println; // []

    let nums = <(1, 20) | count | array::of_list;
    <nums | array::length | println; // 20
    <nums
        | fmt
        | println; // [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
    <mu i64 {
        out <= <(nums, 0) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 1
    <mu i64 {
        out <= <(nums, 3) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 4
    <mu i64 {
        out <= <(nums, 4) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 5
    <mu i64 {
        out <= <(nums, 15) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 16
    <mu i64 {
        out <= <(nums, 16) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 17
    <mu i64 {
        out <= <(nums, 19) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 20
    <mu i64 {
        out <= <(nums, 20) | array::get | (out & select String { _ => <-1 | out> })>,
    } | println; // -1
    <mu i64 {
        out <= <(nums, -1) | array::get | (out & select String { _ => <-1 | out> })>,
    } | println; // -1

    let changed = mu array::Array<i64> {
        out <= <(nums, 16, 100) | array::update | (out & select String { _ => <nums | out> })>,
    };
    <mu i64 {
        out <= <(changed, 16) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 100
    <mu i64 {
        out <= <(nums, 16) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 17
    <changed | array::to_list | fmt | println;

    // One past the next full level: 64 elements fill three levels of branches.
    let more = <(1, 65) | count | array::of_list;
    <more | array::length | println; // 65
    <mu i64 {
        out <= <(more, 0) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 1
    <mu i64 {
        out <= <(more, 63) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 64
    <mu i64 {
        out <= <(more, 64) | array::get | (out & select String { _ => <0 | out> })>,
    } | println; // 65

    <0 | exit>
}
