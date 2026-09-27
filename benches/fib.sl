// Naive double recursion. The checksum is fib(N).

def N: i64 = 22;

func fib(n: i64) -> i64 {
    of (<(n, 1) | le) {
        True => n,
        _ => {
            let a = <(<(n, 1) | sub) | fib;
            let b = <(<(n, 2) | sub) | fib;
            <(a, b) | add
        },
    }
}

proc main | (exit: i32) / {IO} {
    <N | fib | println;
    <0 | exit>
}
