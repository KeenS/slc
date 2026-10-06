// Returning functions, a lambda, and a generic.

func double(n: i64) -> i64 {
    <(n, 2) | mul
}

func greet(name: String) -> String {
    <("Hello, ", name) | add
}

func id<+T>(x: T) -> T { x }

proc main | (exit: i32) / {IO} {
    <21 | double | println;
    <"SLC" | greet | println;
    let square = fn(n: i64) -> i64 { <(n, n) | mul };
    <6 | square | println;
    <7 | id | println;
    <"seven" | id | str_len | println;
    <0 | exit>
}
