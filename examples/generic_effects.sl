effect Reader<+T> { fn read() -> T; }

fn get<+T>() -> T / {Reader<T>} { read() }

command main | (exit: i32) / {IO} {
    let number = handle get() { read(): resume => <42 | resume };
    let text = handle get() { read(): resume => <"hello" | resume };
    <number | println;
    <text | println;

    let stored: Handler<i64, i64, {Reader<i64>}, {}> = handler Reader {
        read(): resume => <7 | resume
    };
    <(with stored handle get()) | println;
    <0 | exit>
}
