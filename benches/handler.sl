// N performances of one operation. The handler resumes with 1 each time, so
// the checksum is N.

def N: i64 = 24000;

hook Ask { func ask() -> i64; }

func burn(n: i64, acc: i64) -> i64 / {Ask} {
    of (<(n, 0) | eq) {
        True => acc,
        _ => {
            let step = ask();
            <(<(n, 1) | sub, <(acc, step) | add) | burn
        },
    }
}

proc main | (exit: i32) / {IO} {
    let total = do (<(N, 0) | burn) hn { ask(): resume => <1 | resume };
    <total | println;
    <0 | exit>
}
