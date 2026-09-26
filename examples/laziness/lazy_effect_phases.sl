use lazy::Lazy;

hook Build { func build() -> i64; }
hook Use { func use_value(input: i64) -> i64; }

data Callback { call: (i64 -> i64 / {Use}) }

func make() -> (i64 -> i64 / {Use}) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add | use_value }
}

func obtain(factory: ((,) -> (i64 -> i64 / {Use}) / {Build})) -> (i64 -> i64 / {Use}) / {Build} {
    <(,) | factory
}

func source() -> Lazy<Callback, {Build}> {
    mu Lazy<Callback, {Build}> {
        force <= {
            let+ built = make();
            <Callback { call: built } | force>
        },
    }
}

proc main | (exit: i32) / {IO} {
    let factory = fn(ignored: (,)) { make() };
    let+ callable = do (<factory | obtain) {
        build(): resume => {
            <"factory build" | println;
            <10 | resume
        },
    };
    <"factory ready" | println;
    let first = do (<1 | callable) {
        use_value(input): resume => {
            <"first use" | println;
            <input | resume
        },
    };
    <first | println;
    let second = do (<2 | callable) {
        use_value(input): resume => {
            <"second use" | println;
            <input | resume
        },
    };
    <second | println;

    let pending = source();
    let ready = do pending.force {
        build(): resume => {
            <"lazy build" | println;
            <20 | resume
        },
    };
    <"lazy ready" | println;
    let third = do (<3 | ready.call) {
        use_value(input): resume => {
            <"third use" | println;
            <input | resume
        },
    };
    <third | println;
    let fourth = do (<4 | ready.call) {
        use_value(input): resume => {
            <"fourth use" | println;
            <input | resume
        },
    };
    <fourth | println;
    let again = do pending.force {
        build(): resume => {
            <"lazy rebuild" | println;
            <30 | resume
        },
    };
    let fifth = do (<5 | again.call) {
        use_value(input): resume => {
            <"fifth use" | println;
            <input | resume
        },
    };
    <fifth | println;
    <0 | exit>
}
