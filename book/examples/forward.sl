// A complete handler, then one that forwards the operation it does not answer.

hook Config {
    func first() -> i64;
    func second() -> i64;
}

func total() -> i64 / {Config} {
    <(first(), second()) | add
}

proc main | (exit: i32) / {IO} {
    let n = do total() hn {
        first(): resume => <1 | resume,
        second(): resume => <2 | resume,
    };
    <n | println;
    let m = do (do total() hn {
        first(): resume => <10 | resume,
        _ => forward,
    }) hn {
        first(): resume => <0 | resume,
        second(): resume => <3 | resume,
    };
    <m | println;
    <0 | exit>
}
