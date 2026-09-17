effect Build { fn build() -> i64; }

fn make(input: i64) -> (i64 -> i64) / {Build} {
    let offset = build();
    fn(value: i64) { <(value, offset) | add }
}

fn ignore(callback: Delayed<(i64 -> i64), {Build}>) -> i64 { 0 }

fn twice(callback: Delayed<(i64 -> i64), {Build}>) -> i64 / {IO} {
    handle (<(<1 | callback, <2 | callback) | add) {
        build(): resume => {
            <"build at demand" | println;
            <10 | resume
        },
    }
}

command main | (exit: i32) / {IO} {
    <1 | make | ignore | println;
    <(<1 | make) | ignore | println;
    <1 | (make | ignore) | println;
    <1 | make | twice | println;
    <(<1 | make) | twice | println;
    <1 | (make | twice) | println;
    <0 | exit>
}
