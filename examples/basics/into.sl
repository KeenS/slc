// `Into<+U>` picks its destination from the type the call is expected to
// produce. The prelude offers it for every pair of integer widths: the
// number is kept when it fits, and a value that does not fit overflows.
// A program can impl the same trait for its own types.

enum Wrap {
    Held(i64),
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

fn as_i64(n: i32) -> i64 {
    <n | into
}
fn as_u8(n: i64) -> u8 {
    <n | into
}
fn as_i32(n: u8) -> i32 {
    <n | into
}
fn as_i64_from_i8(n: i8) -> i64 {
    <n | into
}

command main | (exit: i32) / {IO} {
    <Held(7) | number | println;
    <Held(7) | text | println;
    <Held(7) | to_text | println;
    <Id { value: 4 } | unwrap_num | println;
    <40000 | as_i64 | println;
    <9 | as_u8 | println;
    <200 | as_i32 | println;
    <-3 | as_i64_from_i8 | println;
    <0 | exit>
}
