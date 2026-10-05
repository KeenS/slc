// Pancake sorting over every permutation of 1..=N. The first entry says
// how many leading pancakes to reverse, and that repeats until the first
// entry is 1. The checksum is the sum of those flip counts. The longest
// counts for N = 1..=7 are 0, 1, 2, 4, 7, 10, 16.

cite list::List;
cite list::List::*;
cite list::append;
cite list::range;
cite list::take;
cite list::drop;
cite list::reverse;

def N: i64 = 6;

func flip_prefix(xs: List<i64>, k: i64) -> List<i64> {
    <(<(<(xs, k) | take) | reverse, <(xs, k) | drop) | append
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
        Nil => <(<rev_prefix | reverse, 0) | flipping,
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
    <(<(1, N) | range, none) | sum_perms | println;
    <0 | exit>
}
