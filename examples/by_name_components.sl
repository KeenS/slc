effect Build { fn build() -> i64; }

data Holder { callback: Delayed<(i64 -> i64), {Build}>, stamp: i64 }

fn make() -> (i64 -> i64) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add }
}

fn stamp() -> i64 / {IO} {
    <"positive field" | println;
    0
}

command main | (exit: i32) / {IO} {
    let saved = Holder { callback: make(), stamp: stamp() };
    let choice: (Delayed<(i64 -> i64), {Build}> | i64) = ::0(make());
    <"stored" | println;

    let alias = saved.callback;
    let first = handle (<1 | alias) {
        build(): resume => { <"record build" | println; <10 | resume }
    };
    <first | println;
    let second = handle (<2 | alias) {
        build(): resume => { <"record build" | println; <20 | resume }
    };
    <second | println;

    let third = handle (match choice {
        ::0(callback) => <3 | callback,
        ::1(value) => value
    }) {
        build(): resume => { <"choice build" | println; <30 | resume }
    };
    <third | println;
    <0 | exit>
}
