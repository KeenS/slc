// Match exhaustiveness checking.
// All enum constructors must be covered (or use `_` as a fallback).

enum Color {
    Red,
    Green,
    Blue,
}

func name(c: Color) -> String {
    of c {
        Red => "red",
        Green => "green",
        Blue => "blue",
    }
}

proc main | (exit: i32) / {IO} {
    <Color::Red | name | println;
    <0 | exit>
}
