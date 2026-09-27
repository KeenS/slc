// The sieve of Eratosthenes through N. Once a prime's square has passed
// N, what remains is prime. The checksum is the sum of the primes in
// 2..=N.

cite list::List;
cite list::List::*;

def N: i64 = 800;

func from_to(i: i64, n: i64) -> List<i64> {
    of (<(i, n) | gt) {
        True => Nil,
        _ => Cons(i, <(<(i, 1) | add, n) | from_to),
    }
}

func drop_mult(p: i64, xs: List<i64>) -> List<i64> {
    of xs {
        Nil => Nil,
        Cons(h, rest) => {
            let tail = <(p, rest) | drop_mult;
            of (<(<(h, p) | rem, 0) | eq) {
                True => tail,
                _ => Cons(h, tail),
            }
        },
    }
}

func sieve(xs: List<i64>, limit: i64) -> List<i64> {
    of xs {
        Nil => Nil,
        Cons(p, rest) => {
            of (<(<(p, p) | mul, limit) | gt) {
                True => xs,
                _ => {
                    let kept = <(p, rest) | drop_mult;
                    Cons(p, <(kept, limit) | sieve)
                },
            }
        },
    }
}

func total(xs: List<i64>, acc: i64) -> i64 {
    of xs {
        Nil => acc,
        Cons(h, rest) => <(rest, <(acc, h) | add) | total,
    }
}

proc main | (exit: i32) / {IO} {
    <(<(<(2, N) | from_to, N) | sieve, 0) | total | println;
    <0 | exit>
}
