effect Build { fn build() -> i64; }
effect Use { fn use_value() -> i64; }

fn make_callback() -> (i64 -> i64) / {Use} {
    let offset = use_value();
    fn(input: i64) { <(input, offset) | add }
}

fn make_bundle() -> (Delayed<(i64 -> i64), {Use}> & i64) / {Build} {
    let value = build();
    (make_callback() & value)
}

command main | (exit: i32) / {IO} {
    let pending = make_bundle();
    <"stored" | println;

    let value = handle pending.1 {
        build(): resume => { <"build for value" | println; <10 | resume }
    };
    <value | println;

    handle {
        pending.0;
        <"projected, not activated" | println;
        (,)
    } {
        build(): resume => { <"build for callback" | println; <20 | resume }
    };

    let result = handle (handle (<1 | pending.0) {
        build(): resume => { <"build again" | println; <30 | resume }
    }) {
        use_value(): resume => { <"use callback" | println; <40 | resume }
    };
    <result | println;
    <0 | exit>
}
