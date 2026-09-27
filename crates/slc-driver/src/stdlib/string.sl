// `string`: a persistent string builder expressed as a menu.
//
// The accumulated text is the menu's closed-over state. `append` takes the
// next piece and answers the next state; `finish` answers the text this
// state holds. A bare `append` is still that function. `push` is the
// Display-driven wrapper. There is no mutable cell or special syntax.

pub menu Builder {
    append(part: String): Builder,
    finish: String,
}

func from(text: String, part: String) -> Builder {
    let text = <(text, part) | add;
    mu Builder {
        append(part): out <= <(text, part) | from | out>,
        finish <= <text | finish>,
    }
}

pub func new() -> Builder {
    mu Builder {
        append(part): out <= <("", part) | from | out>,
        finish <= <"" | finish>,
    }
}

// `Display` makes the builder useful for values as well as literal text.
pub func push<+T: Display>(builder: Builder, value: T) -> Builder {
    <value | fmt | builder.append
}
