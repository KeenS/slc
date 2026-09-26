func returning_sink(value: i64) -> (,) / {IO} {
    <value | println
}

func printing_consumer(exit: -i32) -> (-i64 / {IO}) {
    mu i64 {
        value => {
            <value | println;
            <0 | exit>
        },
    }
}

proc main | (exit: -i32) / {IO} {
    <1 | returning_sink;
    <"returned from sink" | println;

    let+ consumer = <exit | printing_consumer;
    <"consumer constructed" | println;
    <2 | consumer>
}
