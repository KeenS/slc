use lazy::Lazy;

effect Build { fn build() -> i64; }
effect Use { fn use_value(input: i64) -> i64; }

data Saved { callback: Delayed<(i64 -> i64 / {Use}), {Build}> }

fn make() -> (i64 -> i64 / {Use}) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add | use_value }
}

fn source() -> Lazy<(i64 -> i64 / {Use}), {Build}> {
    mu Lazy<(i64 -> i64 / {Use}), {Build}> {
        force <= { let+ ready = make(); <ready | force> },
    }
}

command main | (exit: i32) / {IO} {
    let saved = Saved { callback: make() };
    let pending = saved.callback;
    let+ ready = handle { let+ value = pending; value } {
        build(): resume => { <"build now" | println; <10 | resume }
    };
    <"ready" | println;
    let first = handle (<1 | ready) {
        use_value(input): resume => { <"use" | println; <input | resume }
    };
    let second = handle (<2 | ready) {
        use_value(input): resume => { <"use" | println; <input | resume }
    };
    <first | println;
    <second | println;

    let explicit = <pending | lazy::of_delayed;
    let round_trip = <explicit | lazy::to_delayed;
    let third = handle (<3 | round_trip) {
        build(): resume => { <"build again" | println; <20 | resume },
        use_value(input): resume => <input | resume,
    };
    <third | println;

    let thunk = source();
    let+ from_lazy = handle thunk.force {
        build(): resume => { <"lazy build" | println; <30 | resume }
    };
    <"lazy ready" | println;
    let fourth = handle (<4 | from_lazy) {
        use_value(input): resume => { <"lazy use" | println; <input | resume }
    };
    <fourth | println;
    <0 | exit>
}
