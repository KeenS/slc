// Force the first N elements of a count and sum them. The checksum is
// 1 + … + N = N * (N + 1) / 2.

cite list::List;
cite list::List::*;

def N: i64 = 600;

func sum(xs: List<i64>) -> i64 {
    of xs {
        Nil => 0,
        Cons(n, rest) => <(n, <rest | sum) | add,
    }
}

proc main | (exit: i32) / {IO} {
    let xs = <(<1 | stream::count_from, N) | stream::take;
    <xs | sum | println;
    <0 | exit>
}
