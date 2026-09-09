// A `mu` declaration with a value parameter and a continuation parameter.

mu echo(x: +i32) | (k: -i32) {
    x @ k
}

mu main() | (exit: -i32) {
    println(mu ask() | (answer: -i32) {
        echo(42, answer)
    });
    0 @ exit
}
