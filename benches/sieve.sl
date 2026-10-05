// The sieve of Eratosthenes through N. Once a prime's square has passed
// N, what remains is prime. The checksum is the sum of the primes in
// 2..=N.

cite list::List;
cite list::List::*;
cite list::range;
cite list::filter;
cite list::sum;

def N: i64 = 800;

func sieve(xs: List<i64>, limit: i64) -> List<i64> {
    of xs {
        Nil => Nil,
        Cons(p, rest) => {
            of (<(<(p, p) | mul, limit) | gt) {
                True => xs,
                _ => {
                    let kept = <(fn(h: i64) { <(<(<(h, p) | rem, 0) | eq) | not }, rest) | filter;
                    Cons(p, <(kept, limit) | sieve)
                },
            }
        },
    }
}

proc main | (exit: i32) / {IO} {
    <(<(<(2, N) | range, N) | sieve) | sum | println;
    <0 | exit>
}
