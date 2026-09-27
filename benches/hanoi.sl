// Towers of Hanoi. Both smaller towers are visited, so every move is
// counted. The checksum is 2^N - 1.

def N: i64 = 15;

func hanoi(n: i64) -> i64 {
    of (<(n, 0) | eq) {
        True => 0,
        _ => {
            let left = <(<(n, 1) | sub) | hanoi;
            let right = <(<(n, 1) | sub) | hanoi;
            <(<(left, right) | add, 1) | add
        },
    }
}

proc main | (exit: i32) / {IO} {
    <N | hanoi | println;
    <0 | exit>
}
