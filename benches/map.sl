// Insert 1..=N into an ordered map, then look each key up. The checksum is
// the sum of the stored values, N * (N + 1) / 2.

def N: i64 = 160;

func fill(m: map::Map<i64, i64>, n: i64) -> map::Map<i64, i64> {
    of (<(n, 0) | eq) {
        True => m,
        _ => {
            let next = <(m, n, n) | map::insert;
            <(next, <(n, 1) | sub) | fill
        },
    }
}

func lookup_sum(m: map::Map<i64, i64>, n: i64, acc: i64) -> i64 {
    of (<(n, 0) | eq) {
        True => acc,
        _ => {
            let found = mu i64 {
                out <= <(m, n) | map::get | (out & mu String { _ => <0 | out> })>,
            };
            <(m, <(n, 1) | sub, <(acc, found) | add) | lookup_sum
        },
    }
}

proc main | (exit: i32) / {IO} {
    let start: map::Map<i64, i64> = map::empty();
    let filled = <(start, N) | fill;
    <(filled, N, 0) | lookup_sum | println;
    <0 | exit>
}
