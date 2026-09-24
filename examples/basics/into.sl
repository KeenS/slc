// A trait may take a type parameter. `Self` is the value that flows in, and
// the parameter is fixed by the type the call is expected to produce, so one
// `into` serves both destinations. A bound carries the arguments too.

enum Wrap {
    Held(i64),
}

trait Into<+U> {
    fn into(self: Self) -> U;
}

impl Into<i64> for Wrap {
    fn into(self: Wrap) -> i64 {
        match self {
            Held(n) => n,
        }
    }
}

impl Into<String> for Wrap {
    fn into(self: Wrap) -> String {
        match self {
            Held(n) => <n | int_to_str,
        }
    }
}

fn number(w: Wrap) -> i64 {
    <w | into
}

fn text(w: Wrap) -> String {
    <w | into
}

fn to_text<+T: Into<String>>(x: T) -> String {
    <x | into
}

data Id<+T> {
    value: T,
}

impl<+T> Into<T> for Id<T> {
    fn into(self: Id<T>) -> T { self.value }
}

fn unwrap_num(x: Id<i64>) -> i64 {
    <x | into
}

command main | (exit: i32) / {IO} {
    <Held(7) | number | println;
    <Held(7) | text | println;
    <Held(7) | to_text | println;
    <Id { value: 4 } | unwrap_num | println;
    <0 | exit>
}
