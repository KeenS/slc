effect Config {
    fn first() -> i64;
    fn second() -> i64;
}

fn total() -> i64 / {Config} {
    <(first(), second()) | add
}

fn override_first<+A, E>(program: ((,) -> A / {Config, ..E})) -> A / {Config, ..E} {
    handle <(,) | program {
        first(): resume => <7 | resume,
        _ => forward,
    }
}

command main | (exit: -i32) / {IO} {
    let complete = handle total() {
        first(): resume => <10 | resume,
        second(): resume => <20 | resume,
    };
    <("complete: ", <complete | int_to_str) | add | println;

    let forwarded = handle (<(fn { total() }) | override_first) {
        first(): resume => <100 | resume,
        second(): resume => <35 | resume,
    };
    <("forwarded: ", <forwarded | int_to_str) | add | println;
    <0 | exit>
}
