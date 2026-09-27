// A menu field may take the values the demand supplies. The parentheses
// are those values, and the colon is still the answer. `append(part)` is
// `<part | builder.append`: the bare demand remains the function.

menu Builder {
    append(part: String): Builder,
    finish: String,
}

menu Pair {
    both(x: i64, y: i64): i64,
}

func from(text: String) -> Builder {
    mu Builder {
        append(part): out <= <(text, part) | add | from | out>,
        finish <= <text | finish>,
    }
}

func start() -> Builder {
    mu Builder {
        append(part) <= <part | from | append>,
        finish <= <"" | finish>,
    }
}

func numbers() -> Pair {
    mu Pair {
        both(x, y): out <= <(x, y) | add | out>,
    }
}

proc main | (exit: i32) / {IO} {
    let b = start();
    let b = b.append("hi");
    let b = <"!" | b.append;
    <b.finish | println;
    <numbers().both(2, 3) | println;
    <0 | exit>
}
