hook Reader<+T> { func read() -> T; }

func get<+T>() -> T / {Reader<T>} { read() }

proc main | (exit: i32) / {IO} {
    let number = do get() hn { read(): resume => <42 | resume };
    let text = do get() hn { read(): resume => <"hello" | resume };
    <number | println;
    <text | println;

    let stored: (i64 hn i64 / {Reader<i64>}) = hn Reader {
        read(): resume => <7 | resume,
    };
    <(do get() stored) | println;
    <0 | exit>
}
