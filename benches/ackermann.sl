// Ackermann A(3, N). The checksum is A(3, N).

def N: i64 = 5;

func ack(m: i64, n: i64) -> i64 {
    of (<(m, 0) | eq) {
        True => <(n, 1) | add,
        _ => {
            of (<(n, 0) | eq) {
                True => <(<(m, 1) | sub, 1) | ack,
                _ => {
                    let inner = <(m, <(n, 1) | sub) | ack;
                    <(<(m, 1) | sub, inner) | ack
                },
            }
        },
    }
}

proc main | (exit: i32) / {IO} {
    <(3, N) | ack | println;
    <0 | exit>
}
