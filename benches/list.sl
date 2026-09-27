// Build a list, map a function over it, and sum. The checksum is the sum of
// 1..=N, doubled: N * (N + 1).

cite list::List;
cite list::List::*;
cite list::map;

def N: i64 = 800;

func build(n: i64) -> List<i64> {
    of (<(n, 0) | eq) {
        True => Nil,
        _ => {
            let rest = <(<(n, 1) | sub) | build;
            Cons(n, rest)
        },
    }
}

func double(n: i64) -> i64 { <(n, 2) | mul }

func sum(xs: List<i64>) -> i64 {
    of xs {
        Nil => 0,
        Cons(n, rest) => <(n, <rest | sum) | add,
    }
}

proc main | (exit: i32) / {IO} {
    let xs = <N | build;
    let ys = <(double, xs) | map;
    <ys | sum | println;
    <0 | exit>
}
