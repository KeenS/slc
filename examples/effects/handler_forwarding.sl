hook Config {
    func first() -> i64;
    func second() -> i64;
}

func total() -> i64 / {Config} {
    <(first(), second()) | add
}

func override_first<+A, E>(program: ((,) -> A / {Config, ..E})) -> A / {Config, ..E} {
    do <(,) | program {
        first(): resume => <7 | resume,
        _ => forward,
    }
}

proc main | (exit: -i32) / {IO} {
    let complete = do total() {
        first(): resume => <10 | resume,
        second(): resume => <20 | resume,
    };
    <("complete: ", <complete | int_to_str) | add | println;

    let forwarded = do (<(fn { total() }) | override_first) {
        first(): resume => <100 | resume,
        second(): resume => <35 | resume,
    };
    <("forwarded: ", <forwarded | int_to_str) | add | println;
    <0 | exit>
}
