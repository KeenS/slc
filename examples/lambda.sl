// Lambda abstraction and immediate application.

command main | (exit: i32) {
    fn(x: i32) -> i32 { x }(42) | println;
    0 | exit⟩
}
