effect Scale {
    fn factor() -> i64;
}

fn scaled(value: i64) -> i64 / {Scale} {
    <(value, factor()) | mul
}

fn numbers() -> stream::Stream<i64, {Scale}> {
    <(scaled, <1 | stream::count_from) | stream::map
}

command main | (exit: -i32) / {IO} {
    let+ source = numbers();
    <"built, not demanded" | println;

    <handle (<(source, 3) | stream::take) {
        factor(): resume => <10 | resume,
    } | println;
    <handle (<(source, 3) | stream::take) {
        factor(): resume => <100 | resume,
    } | println;

    let+ sequence = <source | seq::of_stream;
    <handle (<(sequence, 2) | seq::take | seq::to_list) {
        factor(): resume => <2 | resume,
    } | println;
    <0 | exit>
}
