data Box<-F> { value: F }
enum Chain<-F> { End, Link(F, Chain<F>) }

fn deliver(out: String) <- i64 {
    select i64 { number => <number | int_to_str | out> }
}

fn first(chain: Chain<(i64 -> String)>) -> String {
    match chain {
        End => "empty",
        Link(stage, rest) => <42 | stage,
    }
}

effect Build { fn build() -> i64; }
effect Use { fn use_value(input: i64) -> i64; }

fn make() -> (i64 -> i64 / {Use}) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add | use_value }
}

command main | (exit: i32) / {IO} {
    let original = Box { value: deliver };
    let boxed: Box<(i64 -> String)> = original;
    <7 | boxed.value | println;

    let chain = Chain::Link(deliver, Chain::Link(deliver, Chain::End));
    <chain | first | println;

    let original = Box { value: make() };
    let delayed: Box<Delayed<(-i64 -> -i64 / {Use}), {Build}>> = original;
    <"stored" | println;
    let+ ready = handle {
        let+ value = delayed.value;
        value
    } {
        build(): resume => {
            <"build now" | println;
            <10 | resume
        },
    };
    <"ready" | println;
    <handle (<1 | ready) {
        use_value(input): resume => {
            <"use" | println;
            <input | resume
        },
    } | println;
    <handle (<2 | delayed.value) {
        build(): resume => {
            <"build again" | println;
            <20 | resume
        },
        use_value(input): resume => {
            <"use" | println;
            <input | resume
        },
    } | println;
    <0 | exit>
}
