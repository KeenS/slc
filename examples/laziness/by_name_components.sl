hook Build { func build() -> i64; }

data Holder { callback: Delayed<(i64 -> i64), {Build}>, stamp: i64 }

func make() -> (i64 -> i64) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add }
}

func stamp() -> i64 / {IO} {
    <"positive field" | println;
    0
}

proc main | (exit: i32) / {IO} {
    let saved = Holder { callback: make(), stamp: stamp() };
    let choice: (Delayed<(i64 -> i64), {Build}> | i64) = ::0(make());
    <"stored" | println;

    let alias = saved.callback;
    let first = do (<1 | alias) {
        build(): resume => {
            <"record build" | println;
            <10 | resume
        },
    };
    <first | println;
    let second = do (<2 | alias) {
        build(): resume => {
            <"record build" | println;
            <20 | resume
        },
    };
    <second | println;

    let third = do (of choice {
        ::0(callback) => <3 | callback,
        ::1(value) => value,
    }) {
        build(): resume => {
            <"choice build" | println;
            <30 | resume
        },
    };
    <third | println;
    <0 | exit>
}
