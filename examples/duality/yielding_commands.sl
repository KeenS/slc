proc choose<E>(input: i64) | (positive: (-i64 / {..E}) & negative: (-i64 / {..E})) / {..E} {
    of (<(input, 0) | gt) {
        True => <input | positive>,
        False => <input | negative>,
    }
}

hook Read { func read() -> i64; }

proc main | (exit: i32) / {IO} {
    <4
        | choose
        | (fn(value: i64) { <(value, 10) | add } & fn(value: i64) { <(value, 10) | sub })
        | println;

    let original = mu i64 {
        out <= <-4 | choose | (mu i64 { value => <(<(value, 10) | add) | out> } & mu i64 {
            value => <(<(value, 10) | sub) | out>,
        })>,
    };
    <original | println;

    let answer = do (<2 | choose | (fn(value: i64) { <(value, read()) | add } & fn(value: i64) {
        0
    })) hn { read(): resume => <40 | resume };
    <answer | println;
    <0 | exit>
}
