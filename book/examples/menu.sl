// A menu field is a function. The demand supplies the arguments.

menu Pair {
    both(x: i64, y: i64): i64,
}

func numbers() -> Pair {
    mu Pair {
        both(x, y): out <= <(x, y) | add | out>,
    }
}

proc main | (exit: i32) / {IO} {
    <numbers().both(2, 3) | println;
    <0 | exit>
}
