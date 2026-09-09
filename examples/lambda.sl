// Lambda abstraction and immediate application.

mu main() | (exit: -i32) {
    println(fn(x: +i32) -> i32 { x }(42));
    0 @ exit
}
