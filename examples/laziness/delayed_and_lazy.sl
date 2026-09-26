cite lazy::Lazy;

hook Build { func build() -> i64; }
hook Use { func use_value(input: i64) -> i64; }

data Saved { callback: (-> (i64 -> i64 / {Use}) / {Build}) }

func make() -> (i64 -> i64 / {Use}) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add | use_value }
}

func source() -> Lazy<(i64 -> i64 / {Use}), {Build}> {
    mu Lazy<(i64 -> i64 / {Use}), {Build}> {
        force <= {
            let+ ready = make();
            <ready | force>
        },
    }
}

proc main | (exit: i32) / {IO} {
    let saved = Saved { callback: make() };
    let pending = saved.callback;
    let+ ready = do {
        let+ value = pending;
        value
    } hn {
        build(): resume => {
            <"build now" | println;
            <10 | resume
        },
    };
    <"ready" | println;
    let first = do (<1 | ready) hn {
        use_value(input): resume => {
            <"use" | println;
            <input | resume
        },
    };
    let second = do (<2 | ready) hn {
        use_value(input): resume => {
            <"use" | println;
            <input | resume
        },
    };
    <first | println;
    <second | println;

    let explicit = <pending | lazy::of_delayed;
    let round_trip = <explicit | lazy::to_delayed;
    let third = do (<3 | round_trip) hn {
        build(): resume => {
            <"build again" | println;
            <20 | resume
        },
        use_value(input): resume => <input | resume,
    };
    <third | println;

    let thunk = source();
    let+ from_lazy = do thunk.force hn {
        build(): resume => {
            <"lazy build" | println;
            <30 | resume
        },
    };
    <"lazy ready" | println;
    let fourth = do (<4 | from_lazy) hn {
        use_value(input): resume => {
            <"lazy use" | println;
            <input | resume
        },
    };
    <fourth | println;
    <0 | exit>
}
