// A tail call that adds 1..=N. The continuation is threaded through, so the
// loop stays a cut into the same consumer. The checksum is N * (N + 1) / 2.

def N: i64 = 30000;

proc count(n: i64, acc: i64) | (done: -i64) {
    of (<(n, 0) | eq) {
        True => <acc | done>,
        _ => <(<(n, 1) | sub, <(acc, n) | add) | count | done>,
    }
}

proc main | (exit: i32) / {IO} {
    let total = mu i64 { done <= <(N, 0) | count | done> };
    <total | println;
    <0 | exit>
}
