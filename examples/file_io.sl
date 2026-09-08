// Reading a file with the IO builtins.

fn main() -> i32 {
    let content = read_file("examples/hello.sl");
    println(content)
}
