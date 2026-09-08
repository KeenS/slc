// A command with a value parameter and a continuation parameter.

command echo(x: +i32, to k: -i32) {
    k(x)
}

fn main() -> i32 {
    42
}
