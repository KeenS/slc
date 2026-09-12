// Match exhaustiveness checking.
// All enum constructors must be covered (or use `_` as a fallback).

enum Color {
    Red,
    Green,
    Blue,
}

fn name(c: Color) -> String {
    match c {
        Red => "red",
        Green => "green",
        Blue => "blue",
    }
}

command main | (exit: -i32) {
    println(name(Color::Red));
    0 | exit
}
