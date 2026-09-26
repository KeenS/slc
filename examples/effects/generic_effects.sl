hook Reader<+T> { func read() -> T; }

func get<+T>() -> T / {Reader<T>} { read() }

proc main | (exit: i32) / {IO} {
    let number = do get() { read(): resume => <42 | resume };
    let text = do get() { read(): resume => <"hello" | resume };
    <number | println;
    <text | println;

    let stored: Handler<i64, i64, {Reader<i64>}, {}> = op Reader {
        read(): resume => <7 | resume,
    };
    <(op stored do get()) | println;
    <0 | exit>
}
