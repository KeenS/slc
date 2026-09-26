hook Build { func build() -> i64; }

func make(input: i64) -> (i64 -> i64) / {Build} {
    let offset = build();
    fn(value: i64) { <(value, offset) | add }
}

func ignore(callback: (-> (i64 -> i64) / {Build})) -> i64 { 0 }

func twice(callback: (-> (i64 -> i64) / {Build})) -> i64 / {IO} {
    do (<(<1 | callback, <2 | callback) | add) hn {
        build(): resume => {
            <"build at demand" | println;
            <10 | resume
        },
    }
}

proc main | (exit: i32) / {IO} {
    <1 | make | ignore | println;
    <(<1 | make) | ignore | println;
    <1 | (make | ignore) | println;
    <1 | make | twice | println;
    <(<1 | make) | twice | println;
    <1 | (make | twice) | println;
    <0 | exit>
}
