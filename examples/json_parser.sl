// A continuation-based JSON parser.
//
// This example deliberately avoids a Result enum. Parsing receives two
// continuations:
//
//   * `ok` receives the parsed JSON text
//   * `err` receives a diagnostic message
//
// `parse_json(source, ok, err)?err` is the selected-continuation form:
// on success it returns to `ok`; on failure it jumps directly to `err`.
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

fn parse_json(
    input: +String,
    ok: +String,
    err: +String,
) -> i64 {
    let start = skip_ws(input, 0);
    if start < str_len(input) {
        let end = parse_value(input, start, ok, err);
        let stopped = skip_ws(input, end);
        if stopped == str_len(input) {
            ok(input[start..end])
        } else {
            err("trailing characters after JSON value")
        }
    } else {
        err("empty input")
    }
}

fn parse_value(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    match input[pos] {
        '0'..='9' | '-' => parse_number(input, pos, ok, err),
        QUOTE => parse_string(input, pos, ok, err),
        OPEN_BRACKET => parse_array(input, pos, ok, err),
        OPEN_BRACE => parse_object(input, pos, ok, err),
        't' => parse_literal(input, pos, "true", ok, err),
        'f' => parse_literal(input, pos, "false", ok, err),
        'n' => parse_literal(input, pos, "null", ok, err),
        _ => err("expected JSON value"),
    }
}

fn parse_number(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let after_sign = if input[pos] == '-' {
        pos + 1
    } else {
        pos
    };
    let integer_end = parse_digits(input, after_sign);
    if integer_end > after_sign {
        parse_number_tail(input, integer_end, ok, err)
    } else {
        err("expected integer part in number")
    }
}

fn parse_number_tail(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let after_fraction = if input[pos] == '.' {
        parse_fraction(input, pos + 1, ok, err)
    } else {
        pos
    };
    parse_exponent(input, after_fraction, ok, err)
}

fn parse_fraction(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    if is_digit(input[pos]) {
        parse_digits(input, pos)
    } else {
        err("expected digit after decimal point")
    }
}

fn parse_exponent(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let ch = input[pos];
    if ch == 'e' || ch == 'E' {
        parse_exponent_tail(input, pos + 1, ok, err)
    } else {
        pos
    }
}

fn parse_exponent_tail(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let after_sign = if input[pos] == '-' || input[pos] == '+' {
        pos + 1
    } else {
        pos
    };
    if is_digit(input[after_sign]) {
        parse_digits(input, after_sign)
    } else {
        err("expected digit in exponent")
    }
}

fn parse_digits(input: +String, pos: +i64) -> i64 {
    if pos < str_len(input) && is_digit(input[pos]) {
        parse_digits(input, pos + 1)
    } else {
        pos
    }
}

fn parse_string(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    parse_string_tail(input, pos + 1, ok, err)
}

fn parse_string_tail(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    if pos >= str_len(input) {
        err("unterminated JSON string")
    } else {
        let ch = input[pos];
        if ch == QUOTE {
            pos + 1
        } else if ch == BACKSLASH {
            parse_escape(input, pos + 1, ok, err)
        } else if ch < ' ' {
            err("raw control character in JSON string")
        } else {
            parse_string_tail(input, pos + 1, ok, err)
        }
    }
}

fn parse_escape(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    match input[pos] {
        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => {
            parse_string_tail(input, pos + 1, ok, err)
        }
        'u' => parse_hex4(input, pos + 1, ok, err),
        _ => err("invalid escape in JSON string"),
    }
}

fn parse_hex4(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let first = parse_hex_digit(input, pos, ok, err);
    let second = parse_hex_digit(input, first, ok, err);
    let third = parse_hex_digit(input, second, ok, err);
    let fourth = parse_hex_digit(input, third, ok, err);
    parse_string_tail(input, fourth, ok, err)
}

fn parse_hex_digit(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    match input[pos] {
        '0'..='9' | 'a'..='f' | 'A'..='F' => pos + 1,
        _ => err("invalid hexadecimal digit in \\u escape"),
    }
}

fn parse_literal(
    input: +String,
    pos: +i64,
    text: +String,
    ok: +String,
    err: +String,
) -> i64 {
    let end = pos + str_len(text);
    if input[pos..end] == text {
        end
    } else {
        err("invalid JSON literal")
    }
}

fn parse_array(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let first = skip_ws(input, pos + 1);
    if input[first] == CLOSE_BRACKET {
        first + 1
    } else {
        parse_array_body(input, first, ok, err)
    }
}

fn parse_array_body(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let value_end = parse_value(input, pos, ok, err);
    let after_value = skip_ws(input, value_end);
    let ch = input[after_value];
    if ch == COMMA {
        let next = skip_ws(input, after_value + 1);
        if input[next] == CLOSE_BRACKET {
            err("trailing comma in array")
        } else {
            parse_array_body(input, next, ok, err)
        }
    } else if ch == CLOSE_BRACKET {
        after_value + 1
    } else {
        err("expected `,` or `]` in array")
    }
}

fn parse_object(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let first = skip_ws(input, pos + 1);
    if input[first] == CLOSE_BRACE {
        first + 1
    } else {
        parse_object_body(input, first, ok, err)
    }
}

fn parse_object_body(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    if input[pos] == QUOTE {
        let key_end = parse_string(input, pos, ok, err);
        let after_key = skip_ws(input, key_end);
        if input[after_key] == COLON {
            let value_start = skip_ws(input, after_key + 1);
            let value_end = parse_value(input, value_start, ok, err);
            let after_value = skip_ws(input, value_end);
            let ch = input[after_value];
            if ch == COMMA {
                let next = skip_ws(input, after_value + 1);
                if input[next] == CLOSE_BRACE {
                    err("trailing comma in object")
                } else {
                    parse_object_body(input, next, ok, err)
                }
            } else if ch == CLOSE_BRACE {
                after_value + 1
            } else {
                err("expected `,` or `}` in object")
            }
        } else {
            err("expected `:` after object key")
        }
    } else {
        err("expected object key")
    }
}

fn main() -> i32 {
    mu(ret: -i32) {
        let source = "{\"name\":\"slant\",\"tags\":[1,2,-3.25],\"active\":true,\"none\":null,\"escaped\":\"a\\\"b\\u0041\"}";
        let ok = fn(value: +String) -> i32 {
            println("parsed: " + value);
            ret(0)
        };
        let err = fn(message: +String) -> i32 {
            println("error: " + message);
            ret(1)
        };
        parse_json(source, ok, err)?err
    }
}
