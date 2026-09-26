hook Build { func build() -> i64; }

func ignore(callback: (-> (i64 -> i64) / {Build})) -> i64 { 0 }

func twice(callback: (-> (i64 -> i64) / {Build})) -> i64 / {Build} {
    <(<1 | callback, <2 | callback) | add
}

func answer() -> i64 / {IO} {
    <"called" | println;
    42
}

proc main | (exit: i32) / {IO} {
    let discard = fn(value) {
        <{
            build();
            value
        } | ignore
    };
    <fn(input: i64) { input } | discard | println;

    let repeat = fn(value) {
        let pending = {
            build();
            value
        };
        do (<pending | twice) hn {
            build(): resume => {
                <"demand" | println;
                <0 | resume
            },
        }
    };
    <fn(input: i64) { input } | repeat | println;

    let factory = answer;
    <"stored" | println;
    <(,) | factory | println;
    <answer() | println;
    <0 | exit>
}
