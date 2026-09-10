// A `command` declaration with a value parameter and a continuation
// parameter, and the `mu` expression that captures one to pass it.

command echo(x: +i32) | (k: -i32) {
    x @ k
}

command main | (exit: -i32) {
    println(mu ask(answer: -i32) {
        echo(42, answer)
    });
    0 @ exit
}
