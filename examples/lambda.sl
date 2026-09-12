// Lambda abstraction and immediate application.

command main | (exit: -i32) {
    println(fn(x: +i32) -> i32 { x }(42));
    0 | exit
}
