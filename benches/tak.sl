// Takeuchi's function on (3N, 2N, N). N = 6 is the famous call,
// tak(18, 12, 6). The checksum is that value.

def N: i64 = 6;

func tak(x: i64, y: i64, z: i64) -> i64 {
    of (<(y, x) | lt) {
        True => {
            let a = <(<(x, 1) | sub, y, z) | tak;
            let b = <(<(y, 1) | sub, z, x) | tak;
            let c = <(<(z, 1) | sub, x, y) | tak;
            <(a, b, c) | tak
        },
        _ => z,
    }
}

proc main | (exit: i32) / {IO} {
    <(<(N, 3) | mul, <(N, 2) | mul, N) | tak | println;
    <0 | exit>
}
