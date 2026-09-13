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
// `(A | B)` is a consumer of `A` and a consumer of `B`, which is what a row of
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

const COMMA: char = ',';
const COLON: char = ':';
const OPEN_BRACKET: char = '[';
const CLOSE_BRACKET: char = ']';
const OPEN_BRACE: char = '{';
const CLOSE_BRACE: char = '}';
const QUOTE: char = '"';
const BACKSLASH: char = '\\';

// Reading one character past the end is not an error while looking ahead:
// the end of the input cannot continue a JSON value. `at` reports a space
// there, which no JSON value accepts and every value may be followed by.
fn at(input: String, pos: i64) -> char {
    match (⟨(pos, (⟨input | str_len)) | lt) { true => {
        (⟨(input, pos) | index)
    }, _ => {
        ' '
    } }
}

fn parse_digits(input: String, pos: i64) -> i64 {
    let more = match (⟨(pos, (⟨input | str_len)) | lt) { true => (⟨(input, pos) | at | is_digit), _ => false };
    match more { true => {
        ⟨(input, (⟨(pos, 1) | add)) | parse_digits
    }, _ => {
        pos
    } }
}

fn is_hex(c: char) -> bool {
    match c {
        '0'..='9' | 'a'..='f' | 'A'..='F' => true,
        _ => false,
    }
}

command parse_json(input: String) | (parsed: String & failed: String) {
    let start = ⟨(input, 0) | skip_ws;
    match (⟨(start, (⟨input | str_len)) | lt) { true => {
        let end = mu { k <= ⟨(input, start) | parse_value | (k & failed)⟩ };
        match (⟨((⟨(input, end) | skip_ws), (⟨input | str_len)) | eq) { true => {
            ⟨(input, start, end) | substring | parsed⟩
        }, _ => {
            ⟨"trailing characters after JSON value" | failed⟩
        } }
    }, _ => {
        ⟨"empty input" | failed⟩
    } }
}

command parse_value(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨(input, pos) | at) {
        '0'..='9' | '-' => ⟨(input, pos) | parse_number | (ok & failed)⟩,
        QUOTE => ⟨(input, pos) | parse_string | (ok & failed)⟩,
        OPEN_BRACKET => ⟨(input, pos) | parse_array | (ok & failed)⟩,
        OPEN_BRACE => ⟨(input, pos) | parse_object | (ok & failed)⟩,
        't' => ⟨(input, pos, "true") | parse_literal | (ok & failed)⟩,
        'f' => ⟨(input, pos, "false") | parse_literal | (ok & failed)⟩,
        'n' => ⟨(input, pos, "null") | parse_literal | (ok & failed)⟩,
        _ => ⟨"expected JSON value" | failed⟩,
    }
}

command parse_number(input: String, pos: i64) | (ok: i64 & failed: String) {
    let after_sign = match (⟨((⟨(input, pos) | at), '-') | eq) { true => {
        (⟨(pos, 1) | add)
    }, _ => {
        pos
    } };
    let integer_end = ⟨(input, after_sign) | parse_digits;
    match (⟨(integer_end, after_sign) | gt) { true => {
        ⟨(input, integer_end) | parse_number_tail | (ok & failed)⟩
    }, _ => {
        ⟨"expected integer part in number" | failed⟩
    } }
}

command parse_number_tail(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨((⟨(input, pos) | at), '.') | eq) { true => {
        let after_fraction = mu { k <= ⟨(input, (⟨(pos, 1) | add)) | parse_fraction | (k & failed)⟩ };
        ⟨(input, after_fraction) | parse_exponent | (ok & failed)⟩
    }, _ => {
        ⟨(input, pos) | parse_exponent | (ok & failed)⟩
    } }
}

command parse_fraction(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨(input, pos) | at | is_digit) { true => {
        ⟨(input, pos) | parse_digits | ok⟩
    }, _ => {
        ⟨"expected digit after decimal point" | failed⟩
    } }
}

command parse_exponent(input: String, pos: i64) | (ok: i64 & failed: String) {
    let ch = ⟨(input, pos) | at;
    let exponent = match (⟨(ch, 'e') | eq) { true => true, _ => (⟨(ch, 'E') | eq) };
    match exponent { true => {
        ⟨(input, (⟨(pos, 1) | add)) | parse_exponent_tail | (ok & failed)⟩
    }, _ => {
        ⟨pos | ok⟩
    } }
}

command parse_exponent_tail(input: String, pos: i64) | (ok: i64 & failed: String) {
    let signed = match (⟨((⟨(input, pos) | at), '-') | eq) { true => true, _ => (⟨((⟨(input, pos) | at), '+') | eq) };
    let after_sign = match signed { true => {
        (⟨(pos, 1) | add)
    }, _ => {
        pos
    } };
    match (⟨(input, after_sign) | at | is_digit) { true => {
        ⟨(input, after_sign) | parse_digits | ok⟩
    }, _ => {
        ⟨"expected digit in exponent" | failed⟩
    } }
}

command parse_string(input: String, pos: i64) | (ok: i64 & failed: String) {
    ⟨(input, (⟨(pos, 1) | add)) | parse_string_tail | (ok & failed)⟩
}

command parse_string_tail(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨(pos, (⟨input | str_len)) | ge) { true => {
        ⟨"unterminated JSON string" | failed⟩
    }, _ => {
        let ch = ⟨(input, pos) | at;
        match (⟨(ch, QUOTE) | eq) { true => {
            ⟨(pos, 1) | add | ok⟩
        }, _ => match (⟨(ch, BACKSLASH) | eq) { true => {
            ⟨(input, (⟨(pos, 1) | add)) | parse_escape | (ok & failed)⟩
        }, _ => match (⟨(ch, ' ') | lt) { true => {
            ⟨"raw control character in JSON string" | failed⟩
        }, _ => {
            ⟨(input, (⟨(pos, 1) | add)) | parse_string_tail | (ok & failed)⟩
        } } } }
    } }
}

command parse_escape(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨(input, pos) | at) {
        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => {
            ⟨(input, (⟨(pos, 1) | add)) | parse_string_tail | (ok & failed)⟩
        }
        'u' => ⟨(input, (⟨(pos, 1) | add)) | parse_hex4 | (ok & failed)⟩,
        _ => ⟨"invalid escape in JSON string" | failed⟩,
    }
}

// Whether `count` hexadecimal digits start at `pos`, looking no further than
// the first that is not one.
fn hex_digits(input: String, pos: i64, count: i64) -> bool {
    match (⟨(count, 0) | eq) { true => true, _ => match (⟨(input, pos) | at | is_hex) {
        true => ⟨(input, (⟨(pos, 1) | add), (⟨(count, 1) | sub)) | hex_digits,
        _ => false,
    } }
}

command parse_hex4(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨(input, pos, 4) | hex_digits) { true => {
        ⟨(input, (⟨(pos, 4) | add)) | parse_string_tail | (ok & failed)⟩
    }, _ => {
        ⟨"invalid hexadecimal digit in \\u escape" | failed⟩
    } }
}

command parse_literal(input: String, pos: i64, text: String) | (ok: i64 & failed: String) {
    let end = (⟨(pos, (⟨text | str_len)) | add);
    // The slice is taken only once it is known to be in range.
    let matches = match (⟨(end, (⟨input | str_len)) | le) { true => (⟨((⟨(input, pos, end) | substring), text) | eq), _ => false };
    match matches { true => {
        ⟨end | ok⟩
    }, _ => {
        ⟨"invalid JSON literal" | failed⟩
    } }
}

command parse_array(input: String, pos: i64) | (ok: i64 & failed: String) {
    let first = ⟨(input, (⟨(pos, 1) | add)) | skip_ws;
    match (⟨((⟨(input, first) | at), CLOSE_BRACKET) | eq) { true => {
        ⟨(first, 1) | add | ok⟩
    }, _ => {
        ⟨(input, first) | parse_array_body | (ok & failed)⟩
    } }
}

command parse_array_body(input: String, pos: i64) | (ok: i64 & failed: String) {
    let value_end = mu { k <= ⟨(input, pos) | parse_value | (k & failed)⟩ };
    let after_value = ⟨(input, value_end) | skip_ws;
    let ch = ⟨(input, after_value) | at;
    match (⟨(ch, COMMA) | eq) { true => {
        let next = ⟨(input, (⟨(after_value, 1) | add)) | skip_ws;
        match (⟨((⟨(input, next) | at), CLOSE_BRACKET) | eq) { true => {
            ⟨"trailing comma in array" | failed⟩
        }, _ => {
            ⟨(input, next) | parse_array_body | (ok & failed)⟩
        } }
    }, _ => match (⟨(ch, CLOSE_BRACKET) | eq) { true => {
        ⟨(after_value, 1) | add | ok⟩
    }, _ => {
        ⟨"expected `,` or `]` in array" | failed⟩
    } } }
}

command parse_object(input: String, pos: i64) | (ok: i64 & failed: String) {
    let first = ⟨(input, (⟨(pos, 1) | add)) | skip_ws;
    match (⟨((⟨(input, first) | at), CLOSE_BRACE) | eq) { true => {
        ⟨(first, 1) | add | ok⟩
    }, _ => {
        ⟨(input, first) | parse_object_body | (ok & failed)⟩
    } }
}

command parse_object_body(input: String, pos: i64) | (ok: i64 & failed: String) {
    match (⟨((⟨(input, pos) | at), QUOTE) | eq) { true => {
        let key_end = mu { k <= ⟨(input, pos) | parse_string | (k & failed)⟩ };
        let after_key = ⟨(input, key_end) | skip_ws;
        match (⟨((⟨(input, after_key) | at), COLON) | eq) { true => {
            let value_end = mu { k <= ⟨(input, ⟨(input, (⟨(after_key, 1) | add)) | skip_ws) | parse_value | (k & failed)⟩ };
            let after_value = ⟨(input, value_end) | skip_ws;
            let ch = ⟨(input, after_value) | at;
            match (⟨(ch, COMMA) | eq) { true => {
                let next = ⟨(input, (⟨(after_value, 1) | add)) | skip_ws;
                match (⟨((⟨(input, next) | at), CLOSE_BRACE) | eq) { true => {
                    ⟨"trailing comma in object" | failed⟩
                }, _ => {
                    ⟨(input, next) | parse_object_body | (ok & failed)⟩
                } }
            }, _ => match (⟨(ch, CLOSE_BRACE) | eq) { true => {
                ⟨(after_value, 1) | add | ok⟩
            }, _ => {
                ⟨"expected `,` or `}` in object" | failed⟩
            } } }
        }, _ => {
            ⟨"expected `:` after object key" | failed⟩
        } }
    }, _ => {
        ⟨"expected object key" | failed⟩
    } }
}

command main | (exit: i32) / {IO} {
    let source = "{\"name\":\"slant\",\"tags\":[1,2,-3.25],\"active\":true,\"none\":null,\"escaped\":\"a\\\"b\\u0041\"}";

    // One consumer per outcome, each ending in a cut against `exit`.
    let parsed = select String {
        value => {
            ⟨("parsed: ", value) | add | println;
            ⟨0 | exit⟩
        },
    };
    let failed = select String {
        message => {
            ⟨("error: ", message) | add | println;
            ⟨1 | exit⟩
        },
    };
    ⟨source | parse_json | (parsed & failed)⟩
}
