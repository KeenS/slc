// N match steps. `n rem 3` picks a variant: 0 adds `n rem 17`, 1 subtracts
// 1, and 2 adds 1. The checksum is the accumulator after counting down from N.

def N: i64 = 16000;

enum Op {
    Add(i64),
    Sub(i64),
    Inc,
}

func classify(n: i64) -> Op {
    let k = <(n, 3) | rem;
    of (<(k, 0) | eq) {
        True => Op::Add(<(n, 17) | rem),
        _ => {
            of (<(k, 1) | eq) {
                True => Op::Sub(1),
                _ => Op::Inc,
            }
        },
    }
}

proc run(n: i64, acc: i64) | (done: -i64) {
    of (<(n, 0) | eq) {
        True => <acc | done>,
        _ => {
            let next = of (<n | classify) {
                Add(v) => <(acc, v) | add,
                Sub(v) => <(acc, v) | sub,
                Inc => <(acc, 1) | add,
            };
            <(<(n, 1) | sub, next) | run | done>
        },
    }
}

proc main | (exit: i32) / {IO} {
    let total = mu i64 { done <= <(N, 0) | run | done> };
    <total | println;
    <0 | exit>
}
