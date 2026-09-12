// A continuation-based JSON parser.
//
// Every parser here is a `command`: it takes values in the first parameter
// group and continuations in the second, and it denotes a command. Nothing
// returns a position — a parser sends its result to one of its continuations,
// one per outcome it can have:
//
//   * `ok` receives the position just past what was parsed
//   * `failed` receives a message, and `parsed` — which only `parse_json`
//     has — receives the text of a complete JSON value
//
// The row is the outcome type. An `enum` of outcomes sent to a single
// continuation says the same thing with a wrapper around it: the consumer of
// `A ⊕ B` is a consumer of `A` and a consumer of `B`, which is what a row of
// two continuations already is. Writing them separately also lets each
// parser's type say which outcomes it actually has — every parser below can
// fail, but only `parse_json` can succeed with a value.
//
// A helper that only computes with values, like `at` or `is_hex`, stays an
// ordinary positive `fn`: it takes no continuation and returns a value.
//
// Where one parser's result feeds the next, a local `mu` captures the
// continuation of the `let` it stands in — the language's `call/cc` — so the
// rest of the parser follows the call instead of nesting inside it. On the
// failure path that continuation is simply never activated, and nothing
// after the `let` runs.
//
// The parser validates the complete input, including trailing characters
// and trailing commas.

const COMMA: +char = ',';
const COLON: +char = ':';
const OPEN_BRACKET: +char = '[';
const CLOSE_BRACKET: +char = ']';
const OPEN_BRACE: +char = '{';
const CLOSE_BRACE: +char = '}';
const QUOTE: +char = '"';
const BACKSLASH: +char = '\\';

// Reading one character past the end is not an error while looking ahead:
// the end of the input cannot continue a JSON value. `at` reports a space
// there, which no JSON value accepts and every value may be followed by.
fn at(input: +String, pos: +i64) -> char {
    if pos < str_len(input) {
        input[pos]
    } else {
        ' '
    }
}

fn parse_digits(input: +String, pos: +i64) -> i64 {
    if pos < str_len(input) && is_digit(at(input, pos)) {
        parse_digits(input, pos + 1)
    } else {
        pos
    }
}

fn is_hex(c: +char) -> bool {
    match c {
        '0'..='9' | 'a'..='f' | 'A'..='F' => true,
        _ => false,
    }
}

command parse_json(input: +String) | (parsed: -String & failed: -String) {
    let start = skip_ws(input, 0);
    if start < str_len(input) {
        let end = mu { k <= parse_value(input, start, k, failed) };
        if skip_ws(input, end) == str_len(input) {
            ⟨input[start..end] | parsed⟩
        } else {
            ⟨"trailing characters after JSON value" | failed⟩
        }
    } else {
        ⟨"empty input" | failed⟩
    }
}

command parse_value(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    match at(input, pos) {
        '0'..='9' | '-' => parse_number(input, pos, ok, failed),
        QUOTE => parse_string(input, pos, ok, failed),
        OPEN_BRACKET => parse_array(input, pos, ok, failed),
        OPEN_BRACE => parse_object(input, pos, ok, failed),
        't' => parse_literal(input, pos, "true", ok, failed),
        'f' => parse_literal(input, pos, "false", ok, failed),
        'n' => parse_literal(input, pos, "null", ok, failed),
        _ => ⟨"expected JSON value" | failed⟩,
    }
}

command parse_number(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    let after_sign = if at(input, pos) == '-' {
        pos + 1
    } else {
        pos
    };
    let integer_end = parse_digits(input, after_sign);
    if integer_end > after_sign {
        parse_number_tail(input, integer_end, ok, failed)
    } else {
        ⟨"expected integer part in number" | failed⟩
    }
}

command parse_number_tail(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    if at(input, pos) == '.' {
        let after_fraction = mu { k <= parse_fraction(input, pos + 1, k, failed) };
        parse_exponent(input, after_fraction, ok, failed)
    } else {
        parse_exponent(input, pos, ok, failed)
    }
}

command parse_fraction(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    if is_digit(at(input, pos)) {
        ⟨parse_digits(input, pos) | ok⟩
    } else {
        ⟨"expected digit after decimal point" | failed⟩
    }
}

command parse_exponent(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    let ch = at(input, pos);
    if ch == 'e' || ch == 'E' {
        parse_exponent_tail(input, pos + 1, ok, failed)
    } else {
        ⟨pos | ok⟩
    }
}

command parse_exponent_tail(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    let after_sign = if at(input, pos) == '-' || at(input, pos) == '+' {
        pos + 1
    } else {
        pos
    };
    if is_digit(at(input, after_sign)) {
        ⟨parse_digits(input, after_sign) | ok⟩
    } else {
        ⟨"expected digit in exponent" | failed⟩
    }
}

command parse_string(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    parse_string_tail(input, pos + 1, ok, failed)
}

command parse_string_tail(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    if pos >= str_len(input) {
        ⟨"unterminated JSON string" | failed⟩
    } else {
        let ch = at(input, pos);
        if ch == QUOTE {
            ⟨pos + 1 | ok⟩
        } else if ch == BACKSLASH {
            parse_escape(input, pos + 1, ok, failed)
        } else if ch < ' ' {
            ⟨"raw control character in JSON string" | failed⟩
        } else {
            parse_string_tail(input, pos + 1, ok, failed)
        }
    }
}

command parse_escape(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    match at(input, pos) {
        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => {
            parse_string_tail(input, pos + 1, ok, failed)
        }
        'u' => parse_hex4(input, pos + 1, ok, failed),
        _ => ⟨"invalid escape in JSON string" | failed⟩,
    }
}

command parse_hex4(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    if is_hex(at(input, pos))
        && is_hex(at(input, pos + 1))
        && is_hex(at(input, pos + 2))
        && is_hex(at(input, pos + 3))
    {
        parse_string_tail(input, pos + 4, ok, failed)
    } else {
        ⟨"invalid hexadecimal digit in \\u escape" | failed⟩
    }
}

command parse_literal(input: +String, pos: +i64, text: +String) | (ok: -i64 & failed: -String) {
    let end = pos + str_len(text);
    if end <= str_len(input) && input[pos..end] == text {
        ⟨end | ok⟩
    } else {
        ⟨"invalid JSON literal" | failed⟩
    }
}

command parse_array(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    let first = skip_ws(input, pos + 1);
    if at(input, first) == CLOSE_BRACKET {
        ⟨first + 1 | ok⟩
    } else {
        parse_array_body(input, first, ok, failed)
    }
}

command parse_array_body(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    let value_end = mu { k <= parse_value(input, pos, k, failed) };
    let after_value = skip_ws(input, value_end);
    let ch = at(input, after_value);
    if ch == COMMA {
        let next = skip_ws(input, after_value + 1);
        if at(input, next) == CLOSE_BRACKET {
            ⟨"trailing comma in array" | failed⟩
        } else {
            parse_array_body(input, next, ok, failed)
        }
    } else if ch == CLOSE_BRACKET {
        ⟨after_value + 1 | ok⟩
    } else {
        ⟨"expected `,` or `]` in array" | failed⟩
    }
}

command parse_object(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    let first = skip_ws(input, pos + 1);
    if at(input, first) == CLOSE_BRACE {
        ⟨first + 1 | ok⟩
    } else {
        parse_object_body(input, first, ok, failed)
    }
}

command parse_object_body(input: +String, pos: +i64) | (ok: -i64 & failed: -String) {
    if at(input, pos) == QUOTE {
        let key_end = mu { k <= parse_string(input, pos, k, failed) };
        let after_key = skip_ws(input, key_end);
        if at(input, after_key) == COLON {
            let value_end = mu { k <= parse_value(input, skip_ws(input, after_key + 1), k, failed) };
            let after_value = skip_ws(input, value_end);
            let ch = at(input, after_value);
            if ch == COMMA {
                let next = skip_ws(input, after_value + 1);
                if at(input, next) == CLOSE_BRACE {
                    ⟨"trailing comma in object" | failed⟩
                } else {
                    parse_object_body(input, next, ok, failed)
                }
            } else if ch == CLOSE_BRACE {
                ⟨after_value + 1 | ok⟩
            } else {
                ⟨"expected `,` or `}` in object" | failed⟩
            }
        } else {
            ⟨"expected `:` after object key" | failed⟩
        }
    } else {
        ⟨"expected object key" | failed⟩
    }
}

command main | (exit: -i32) {
    let source = "{\"name\":\"slant\",\"tags\":[1,2,-3.25],\"active\":true,\"none\":null,\"escaped\":\"a\\\"b\\u0041\"}";

    // One consumer per outcome, each ending in a cut against `exit`.
    let parsed = select +String {
        value => {
            println("parsed: " + value);
            ⟨0 | exit⟩
        },
    };
    let failed = select +String {
        message => {
            println("error: " + message);
            ⟨1 | exit⟩
        },
    };
    parse_json(source, parsed, failed)
}
