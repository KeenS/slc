// A continuation-based JSON parser.
//
// Every parser here is a `mu`: it takes values in the first parameter group
// and continuations in the second, and it denotes a command. Nothing returns
// a position — a parser sends its result to one of its continuations:
//
//   * `ok` receives the position just past what was parsed
//   * `report` receives a `ParseResult` describing the whole outcome
//
// `report` is built by `select`, the negative additive dual of the enum:
// activating it with a variant dispatches to that variant's arm and binds the
// payload for it. Success and failure meet in one place — at the bottom of
// this file, where the parser is called — instead of being carried side by
// side through every parser as two continuations.
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

enum ParseResult {
    Parsed(String),
    Failed(String),
}

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

mu parse_json(input: +String) | (report: -ParseResult) {
    let start = skip_ws(input, 0);
    if start < str_len(input) {
        let end = mu value | (k) {
            parse_value(input, start, k, report)
        };
        if skip_ws(input, end) == str_len(input) {
            ParseResult::Parsed(input[start..end]) @ report
        } else {
            ParseResult::Failed("trailing characters after JSON value") @ report
        }
    } else {
        ParseResult::Failed("empty input") @ report
    }
}

mu parse_value(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    match at(input, pos) {
        '0'..='9' | '-' => parse_number(input, pos, ok, report),
        QUOTE => parse_string(input, pos, ok, report),
        OPEN_BRACKET => parse_array(input, pos, ok, report),
        OPEN_BRACE => parse_object(input, pos, ok, report),
        't' => parse_literal(input, pos, "true", ok, report),
        'f' => parse_literal(input, pos, "false", ok, report),
        'n' => parse_literal(input, pos, "null", ok, report),
        _ => ParseResult::Failed("expected JSON value") @ report,
    }
}

mu parse_number(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    let after_sign = if at(input, pos) == '-' {
        pos + 1
    } else {
        pos
    };
    let integer_end = parse_digits(input, after_sign);
    if integer_end > after_sign {
        parse_number_tail(input, integer_end, ok, report)
    } else {
        ParseResult::Failed("expected integer part in number") @ report
    }
}

mu parse_number_tail(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    if at(input, pos) == '.' {
        let after_fraction = mu fraction | (k) {
            parse_fraction(input, pos + 1, k, report)
        };
        parse_exponent(input, after_fraction, ok, report)
    } else {
        parse_exponent(input, pos, ok, report)
    }
}

mu parse_fraction(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    if is_digit(at(input, pos)) {
        parse_digits(input, pos) @ ok
    } else {
        ParseResult::Failed("expected digit after decimal point") @ report
    }
}

mu parse_exponent(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    let ch = at(input, pos);
    if ch == 'e' || ch == 'E' {
        parse_exponent_tail(input, pos + 1, ok, report)
    } else {
        pos @ ok
    }
}

mu parse_exponent_tail(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    let after_sign = if at(input, pos) == '-' || at(input, pos) == '+' {
        pos + 1
    } else {
        pos
    };
    if is_digit(at(input, after_sign)) {
        parse_digits(input, after_sign) @ ok
    } else {
        ParseResult::Failed("expected digit in exponent") @ report
    }
}

mu parse_string(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    parse_string_tail(input, pos + 1, ok, report)
}

mu parse_string_tail(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    if pos >= str_len(input) {
        ParseResult::Failed("unterminated JSON string") @ report
    } else {
        let ch = at(input, pos);
        if ch == QUOTE {
            pos + 1 @ ok
        } else if ch == BACKSLASH {
            parse_escape(input, pos + 1, ok, report)
        } else if ch < ' ' {
            ParseResult::Failed("raw control character in JSON string") @ report
        } else {
            parse_string_tail(input, pos + 1, ok, report)
        }
    }
}

mu parse_escape(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    match at(input, pos) {
        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => {
            parse_string_tail(input, pos + 1, ok, report)
        }
        'u' => parse_hex4(input, pos + 1, ok, report),
        _ => ParseResult::Failed("invalid escape in JSON string") @ report,
    }
}

mu parse_hex4(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    if is_hex(at(input, pos))
        && is_hex(at(input, pos + 1))
        && is_hex(at(input, pos + 2))
        && is_hex(at(input, pos + 3))
    {
        parse_string_tail(input, pos + 4, ok, report)
    } else {
        ParseResult::Failed("invalid hexadecimal digit in \\u escape") @ report
    }
}

mu parse_literal(input: +String, pos: +i64, text: +String) | (ok: -i64, report: -ParseResult) {
    let end = pos + str_len(text);
    if end <= str_len(input) && input[pos..end] == text {
        end @ ok
    } else {
        ParseResult::Failed("invalid JSON literal") @ report
    }
}

mu parse_array(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    let first = skip_ws(input, pos + 1);
    if at(input, first) == CLOSE_BRACKET {
        first + 1 @ ok
    } else {
        parse_array_body(input, first, ok, report)
    }
}

mu parse_array_body(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    let value_end = mu value | (k) {
        parse_value(input, pos, k, report)
    };
    let after_value = skip_ws(input, value_end);
    let ch = at(input, after_value);
    if ch == COMMA {
        let next = skip_ws(input, after_value + 1);
        if at(input, next) == CLOSE_BRACKET {
            ParseResult::Failed("trailing comma in array") @ report
        } else {
            parse_array_body(input, next, ok, report)
        }
    } else if ch == CLOSE_BRACKET {
        after_value + 1 @ ok
    } else {
        ParseResult::Failed("expected `,` or `]` in array") @ report
    }
}

mu parse_object(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    let first = skip_ws(input, pos + 1);
    if at(input, first) == CLOSE_BRACE {
        first + 1 @ ok
    } else {
        parse_object_body(input, first, ok, report)
    }
}

mu parse_object_body(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    if at(input, pos) == QUOTE {
        let key_end = mu key | (k) {
            parse_string(input, pos, k, report)
        };
        let after_key = skip_ws(input, key_end);
        if at(input, after_key) == COLON {
            let value_end = mu value | (k) {
                parse_value(input, skip_ws(input, after_key + 1), k, report)
            };
            let after_value = skip_ws(input, value_end);
            let ch = at(input, after_value);
            if ch == COMMA {
                let next = skip_ws(input, after_value + 1);
                if at(input, next) == CLOSE_BRACE {
                    ParseResult::Failed("trailing comma in object") @ report
                } else {
                    parse_object_body(input, next, ok, report)
                }
            } else if ch == CLOSE_BRACE {
                after_value + 1 @ ok
            } else {
                ParseResult::Failed("expected `,` or `}` in object") @ report
            }
        } else {
            ParseResult::Failed("expected `:` after object key") @ report
        }
    } else {
        ParseResult::Failed("expected object key") @ report
    }
}

mu main | (exit: -i32) {
    let source = "{\"name\":\"slant\",\"tags\":[1,2,-3.25],\"active\":true,\"none\":null,\"escaped\":\"a\\\"b\\u0041\"}";

    // One consumer for the whole outcome: an arm per variant, each binding
    // that variant's payload and ending in a cut against `exit`.
    parse_json(source, select ParseResult {
        Parsed(value) <= {
            println("parsed: " + value);
            0 @ exit
        },
        Failed(message) <= {
            println("error: " + message);
            1 @ exit
        },
    })
}
