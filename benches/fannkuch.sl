// Pancake sorting over every permutation of 1..=N. The first entry says
// how many leading pancakes to reverse, and that repeats until the first
// entry is 1. The checksum is the sum of those flip counts. The longest
// counts for N = 1..=7 are 0, 1, 2, 4, 7, 10, 16.

cite list::List;
cite list::List::*;
cite list::append;

def N: i64 = 6;

func iota(i: i64, n: i64) -> List<i64> {
    of (<(i, n) | gt) {
        True => Nil,
        _ => Cons(i, <(<(i, 1) | add, n) | iota),
    }
}

func take(xs: List<i64>, n: i64) -> List<i64> {
    of (<(n, 0) | le) {
        True => Nil,
        _ => {
            of xs {
                Nil => Nil,
                Cons(h, rest) => Cons(h, <(rest, <(n, 1) | sub) | take),
            }
        },
    }
}

func drop(xs: List<i64>, n: i64) -> List<i64> {
    of (<(n, 0) | le) {
        True => xs,
        _ => {
            of xs {
                Nil => Nil,
                Cons(_, rest) => <(rest, <(n, 1) | sub) | drop,
            }
        },
    }
}

func rev_onto(xs: List<i64>, acc: List<i64>) -> List<i64> {
    of xs {
        Nil => acc,
        Cons(h, rest) => <(rest, Cons(h, acc)) | rev_onto,
    }
}

func flip_prefix(xs: List<i64>, k: i64) -> List<i64> {
    <(<(xs, k) | take, <(xs, k) | drop) | rev_onto
}

func flipping(p: List<i64>, acc: i64) -> i64 {
    of p {
        Nil => acc,
        Cons(k, _) => {
            of (<(k, 1) | eq) {
                True => acc,
                _ => <(<(p, k) | flip_prefix, <(acc, 1) | add) | flipping,
            }
        },
    }
}

func sum_perms(pool: List<i64>, rev_prefix: List<i64>) -> i64 {
    of pool {
        Nil => <(<(rev_prefix, Nil) | rev_onto, 0) | flipping,
        _ => <(pool, rev_prefix, Nil) | pick,
    }
}

func pick(pool: List<i64>, rev_prefix: List<i64>, before: List<i64>) -> i64 {
    of pool {
        Nil => 0,
        Cons(h, after) => {
            let chosen = <(<(before, after) | append, Cons(h, rev_prefix)) | sum_perms;
            let more = <(after, rev_prefix, Cons(h, before)) | pick;
            <(chosen, more) | add
        },
    }
}

proc main | (exit: i32) / {IO} {
    let none: List<i64> = Nil;
    <(<(1, N) | iota, none) | sum_perms | println;
    <0 | exit>
}
