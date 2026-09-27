// Push 1..=N onto an array, then sum by index. The checksum is
// N * (N + 1) / 2. A missing index adds -1.

def N: i64 = 450;

func push_up(a: array::Array<i64>, i: i64, n: i64) -> array::Array<i64> {
    of (<(i, n) | gt) {
        True => a,
        _ => {
            let next = <(a, i) | array::push;
            <(next, <(i, 1) | add, n) | push_up
        },
    }
}

func sum_at(a: array::Array<i64>, i: i64, acc: i64) -> i64 {
    of (<(i, 0) | lt) {
        True => acc,
        _ => {
            let found = mu i64 {
                out <= <(a, i) | array::get | (out & mu String { _ => <-1 | out> })>,
            };
            <(a, <(i, 1) | sub, <(acc, found) | add) | sum_at
        },
    }
}

proc main | (exit: i32) / {IO} {
    let start: array::Array<i64> = array::empty();
    let built = <(start, 1, N) | push_up;
    <(built, <(N, 1) | sub, 0) | sum_at | println;
    <0 | exit>
}
