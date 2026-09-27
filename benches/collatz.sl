// Hailstone steps for every integer from 1 to N. An even number is
// halved and an odd number becomes 3n + 1, until it reaches 1. The
// checksum is the sum of those step counts.

def N: i64 = 500;

func stepping(n: i64, acc: i64) -> i64 {
    of (<(n, 1) | eq) {
        True => acc,
        _ => {
            of (<(<(n, 2) | rem, 0) | eq) {
                True => <(<(n, 2) | div, <(acc, 1) | add) | stepping,
                _ => <(<(<(n, 3) | mul, 1) | add, <(acc, 1) | add) | stepping,
            }
        },
    }
}

func total(n: i64, acc: i64) -> i64 {
    of (<(n, 0) | eq) {
        True => acc,
        _ => <(<(n, 1) | sub, <(acc, <(n, 0) | stepping) | add) | total,
    }
}

proc main | (exit: i32) / {IO} {
    <(N, 0) | total | println;
    <0 | exit>
}
