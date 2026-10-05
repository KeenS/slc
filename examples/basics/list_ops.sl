// Eager list operations: an inclusive range, then filter, reverse, take,
// drop, sum, and fold.

cite list::List;
cite list::List::*;
cite list::range;
cite list::filter;
cite list::fold;
cite list::reverse;
cite list::take;
cite list::drop;
cite list::sum;
cite list::length;

func add_i64(acc: i64, n: i64) -> i64 {
    <(acc, n) | add
}

func even(n: i64) -> Bool {
    of (<(n, 2) | rem) {
        0 => True,
        _ => False,
    }
}

proc main | (exit: i32) / {IO} {
    let xs = <(1, 4) | range;
    <xs | println;
    <(even, xs) | filter | println;
    <xs | reverse | println;
    <(xs, 2) | take | println;
    <(xs, 2) | drop | println;
    <xs | sum | println;
    <(xs, 0, add_i64) | fold | println;
    <(3, 1) | range | println;
    <(xs, 0) | take | println;
    <(xs, 0) | drop | println;
    <(<(9223372036854775807, 9223372036854775807) | range) | length | println;
    <0 | exit>
}
