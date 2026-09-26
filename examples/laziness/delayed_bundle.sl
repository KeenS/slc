hook Build { func build() -> i64; }
hook Use { func use_value() -> i64; }

func make_callback() -> (i64 -> i64) / {Use} {
    let offset = use_value();
    fn(input: i64) { <(input, offset) | add }
}

func make_bundle() -> ((-> (i64 -> i64) / {Use}) & i64) / {Build} {
    let value = build();
    (make_callback() & value)
}

proc main | (exit: i32) / {IO} {
    let pending = make_bundle();
    <"stored" | println;

    let value = do pending.1 hn {
        build(): resume => {
            <"build for value" | println;
            <10 | resume
        },
    };
    <value | println;

    do {
        pending.0;
        <"projected, not activated" | println;
        (,)
    } hn {
        build(): resume => {
            <"build for callback" | println;
            <20 | resume
        },
    };

    let result = do (do (<1 | pending.0) hn {
        build(): resume => {
            <"build again" | println;
            <30 | resume
        },
    }) hn {
        use_value(): resume => {
            <"use callback" | println;
            <40 | resume
        },
    };
    <result | println;
    <0 | exit>
}
