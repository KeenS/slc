// Quicksort of N values from x0 = 1, x(i + 1) = (17 * x(i) + 13) rem 10007.
// The sorted values are mixed from 0 by acc = xor(wrapping_mul(acc, 31), value).
// The checksum is that mix, so the order matters.

cite list::List;
cite list::List::*;
cite list::append;

def N: i64 = 350;

func gen(n: i64, seed: i64) -> List<i64> {
    of (<(n, 0) | eq) {
        True => Nil,
        _ => {
            let next = <(<(<(seed, 17) | mul, 13) | add, 10007) | rem;
            Cons(next, <(<(n, 1) | sub, next) | gen)
        },
    }
}

func below(pivot: i64, xs: List<i64>) -> List<i64> {
    of xs {
        Nil => Nil,
        Cons(h, rest) => {
            let tail = <(pivot, rest) | below;
            of (<(h, pivot) | lt) {
                True => Cons(h, tail),
                _ => tail,
            }
        },
    }
}

func not_below(pivot: i64, xs: List<i64>) -> List<i64> {
    of xs {
        Nil => Nil,
        Cons(h, rest) => {
            let tail = <(pivot, rest) | not_below;
            of (<(h, pivot) | lt) {
                True => tail,
                _ => Cons(h, tail),
            }
        },
    }
}

func qsort(xs: List<i64>) -> List<i64> {
    of xs {
        Nil => Nil,
        Cons(pivot, rest) => {
            let left = <(<(pivot, rest) | below) | qsort;
            let right = <(<(pivot, rest) | not_below) | qsort;
            <(left, Cons(pivot, right)) | append
        },
    }
}

func mix(xs: List<i64>, acc: i64) -> i64 {
    of xs {
        Nil => acc,
        Cons(h, rest) => <(rest, <(<(acc, 31) | wrapping_mul, h) | xor) | mix,
    }
}

proc main | (exit: i32) / {IO} {
    <(<(<(N, 1) | gen) | qsort, 0) | mix | println;
    <0 | exit>
}
