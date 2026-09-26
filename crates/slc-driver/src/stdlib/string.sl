// `string`: a persistent string builder expressed as a menu.
//
// The accumulated text is the menu's closed-over state. Asking for `append`
// gives a function from String to the next state; asking for `finish` gives
// the text held by the current state. `push` is the Display-driven wrapper.
// There is no mutable cell or special syntax.

pub menu Builder {
    append: (String -> Builder),
    finish: String,
}

func from(text: String, part: String) -> Builder {
    let text = <(text, part) | add;
    mu Builder {
        append <= <fn(part: String) { <(text, part) | from } | append>,
        finish <= <text | finish>,
    }
}

pub func new() -> Builder {
    mu Builder {
        append <= <fn(part: String) { <("", part) | from } | append>,
        finish <= <"" | finish>,
    }
}

// `Display` makes the builder useful for values as well as literal text.
pub func push<+T: Display>(builder: Builder, value: T) -> Builder {
    <value | fmt | builder.append
}
