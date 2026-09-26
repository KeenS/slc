hook Scale {
    func factor() -> i64;
}

func scaled(value: i64) -> i64 / {Scale} {
    <(value, factor()) | mul
}

func numbers() -> stream::Stream<i64, {Scale}> {
    <(scaled, <1 | stream::count_from) | stream::map
}

proc main | (exit: -i32) / {IO} {
    let+ source = numbers();
    <"built, not demanded" | println;

    <do (<(source, 3) | stream::take) {
        factor(): resume => <10 | resume,
    } | println;
    <do (<(source, 3) | stream::take) {
        factor(): resume => <100 | resume,
    } | println;

    let+ sequence = <source | seq::of_stream;
    <do (<(sequence, 2) | seq::take | seq::to_list) {
        factor(): resume => <2 | resume,
    } | println;
    <0 | exit>
}
