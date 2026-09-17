command choose<E>(input: i64) | (positive: (-i64 / {..E}) & negative: (-i64 / {..E})) / {..E} {
    match (<(input, 0) | gt) {
        True => <input | positive>,
        False => <input | negative>,
    }
}

effect Read { fn read() -> i64; }

command main | (exit: i32) / {IO} {
    <4
        | choose
        | (fn(value: i64) { <(value, 10) | add } & fn(value: i64) { <(value, 10) | sub })
        | println;

    let original = mu i64 {
        out <= <-4 | choose | (select i64 { value => <(<(value, 10) | add) | out> } & select i64 {
            value => <(<(value, 10) | sub) | out>,
        })>,
    };
    <original | println;

    let answer = handle (<2 | choose | (fn(value: i64) { <(value, read()) | add } & fn(value: i64) {
        0
    })) { read(): resume => <40 | resume };
    <answer | println;
    <0 | exit>
}
