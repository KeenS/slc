// A `proc` declaration with a value parameter and a continuation
// parameter, and the `mu` expression that captures one to pass it.

proc echo(x: i32) | (k: i32) {
    <x | k>
}

proc main | (exit: i32) / {IO} {
    <mu i32 { answer <= <42 | echo | answer> } | println;
    <0 | exit>
}
