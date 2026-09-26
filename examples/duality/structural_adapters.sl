data Box<-F> { value: F }
enum Chain<-F> { End, Link(F, Chain<F>) }

func deliver(out: String) <- i64 {
    mu i64 { number => <number | int_to_str | out> }
}

func first(chain: Chain<(i64 -> String)>) -> String {
    of chain {
        End => "empty",
        Link(stage, rest) => <42 | stage,
    }
}

hook Build { func build() -> i64; }
hook Use { func use_value(input: i64) -> i64; }

func make() -> (i64 -> i64 / {Use}) / {Build} {
    let offset = build();
    fn(input: i64) { <(input, offset) | add | use_value }
}

proc main | (exit: i32) / {IO} {
    let original = Box { value: deliver };
    let boxed: Box<(i64 -> String)> = original;
    <7 | boxed.value | println;

    let chain = Chain::Link(deliver, Chain::Link(deliver, Chain::End));
    <chain | first | println;

    let original = Box { value: make() };
    let delayed: Box<Delayed<(-i64 -> -i64 / {Use}), {Build}>> = original;
    <"stored" | println;
    let+ ready = do {
        let+ value = delayed.value;
        value
    } {
        build(): resume => {
            <"build now" | println;
            <10 | resume
        },
    };
    <"ready" | println;
    <do (<1 | ready) {
        use_value(input): resume => {
            <"use" | println;
            <input | resume
        },
    } | println;
    <do (<2 | delayed.value) {
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
