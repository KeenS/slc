// Insert 1..=N into a hash map, then look each key up. The checksum is the
// sum of the stored values, N * (N + 1) / 2.

def N: i64 = 200;

func fill(m: hashmap::HashMap<i64, i64>, n: i64) -> hashmap::HashMap<i64, i64> {
    of (<(n, 0) | eq) {
        True => m,
        _ => {
            let next = <(m, n, n) | hashmap::insert;
            <(next, <(n, 1) | sub) | fill
        },
    }
}

func lookup_sum(m: hashmap::HashMap<i64, i64>, n: i64, acc: i64) -> i64 {
    of (<(n, 0) | eq) {
        True => acc,
        _ => {
            let found = mu i64 {
                out <= <(m, n) | hashmap::get | (out & mu String { _ => <0 | out> })>,
            };
            <(m, <(n, 1) | sub, <(acc, found) | add) | lookup_sum
        },
    }
}

proc main | (exit: i32) / {IO} {
    let start: hashmap::HashMap<i64, i64> = hashmap::empty();
    let filled = <(start, N) | fill;
    <(filled, N, 0) | lookup_sum | println;
    <0 | exit>
}
