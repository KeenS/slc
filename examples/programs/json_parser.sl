// A continuation-based JSON parser.
//
// Every parser here is a `proc`: it takes values in the first parameter
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
// ordinary positive `func`: it takes no continuation and returns a value.
//
// Where one parser's result feeds the next, a local `mu` captures the
// continuation of the `let` it stands in — the language's `call/cc` — so the
// rest of the parser follows the call instead of nesting inside it. On the
// failure path that continuation is simply never activated, and nothing
// after the `let` runs.
//
// The same thing, without a `mu`, is an `of` that leaves on one arm. A cut
// has type `(;)` and fixes nothing about the match, so the arm that
// continues is `(,)` and the statements after the `of` are the rest of that
// path. The cut never reaches them.
//
// The parser validates the complete input, including trailing characters
// and trailing commas.

def COMMA: char = ',';
def COLON: char = ':';
def OPEN_BRACKET: char = '[';
def CLOSE_BRACKET: char = ']';
def OPEN_BRACE: char = '{';
def CLOSE_BRACE: char = '}';
def QUOTE: char = '"';
def BACKSLASH: char = '\\';

// Reading one character past the end is not an error while looking ahead:
// the end of the input cannot continue a JSON value. `at` reports a space
// there, which no JSON value accepts and every value may be followed by.
func at(input: String, pos: i64) -> char {
    of (<(pos, <input | str_len) | lt) {
        True => <(input, pos) | index,
        False => ' ',
    }
}

func parse_digits(input: String, pos: i64) -> i64 {
    let more = of (<(pos, <input | str_len) | lt) {
        True => <(input, pos) | at | is_digit,
        False => False,
    };
    of more {
        True => <(input, <(pos, 1) | add) | parse_digits,
        False => pos,
    }
}

func is_hex(c: char) -> Bool {
    of c {
        '0'..='9' | 'a'..='f' | 'A'..='F' => True,
        _ => False,
    }
}

proc parse_json<E>(input: String) | (
    parsed: (-String / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let start = <(input, 0) | skip_ws;
    of (<(start, <input | str_len) | ge) {
        True => <"empty input" | failed>,
        False => (,),
    };
    let end = mu { k <= <(input, start) | parse_value | (k & failed)> };
    of (<(<(input, end) | skip_ws, <input | str_len) | eq) {
        True => <(input, start, end) | substring | parsed>,
        False => <"trailing characters after JSON value" | failed>,
    }
}

proc parse_value<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(input, pos) | at) {
        '0'..='9' | '-' => <(input, pos) | parse_number | (ok & failed)>,
        QUOTE => <(input, pos) | parse_string | (ok & failed)>,
        OPEN_BRACKET => <(input, pos) | parse_array | (ok & failed)>,
        OPEN_BRACE => <(input, pos) | parse_object | (ok & failed)>,
        't' => <(input, pos, "true") | parse_literal | (ok & failed)>,
        'f' => <(input, pos, "false") | parse_literal | (ok & failed)>,
        'n' => <(input, pos, "null") | parse_literal | (ok & failed)>,
        _ => <"expected JSON value" | failed>,
    }
}

proc parse_number<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let after_sign = of (<(<(input, pos) | at, '-') | eq) {
        True => <(pos, 1) | add,
        False => pos,
    };
    let integer_end = <(input, after_sign) | parse_digits;
    of (<(integer_end, after_sign) | gt) {
        True => <(input, integer_end) | parse_number_tail | (ok & failed)>,
        False => <"expected integer part in number" | failed>,
    }
}

proc parse_number_tail<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(<(input, pos) | at, '.') | ne) {
        True => <(input, pos) | parse_exponent | (ok & failed)>,
        False => (,),
    };
    let after_fraction = mu {
        k <= <(input, <(pos, 1) | add) | parse_fraction | (k & failed)>,
    };
    <(input, after_fraction) | parse_exponent | (ok & failed)>
}

proc parse_fraction<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(input, pos) | at | is_digit) {
        True => <(input, pos) | parse_digits | ok>,
        False => <"expected digit after decimal point" | failed>,
    }
}

proc parse_exponent<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let ch = <(input, pos) | at;
    let exponent = of (<(ch, 'e') | eq) { True => True, False => <(ch, 'E') | eq };
    of exponent {
        True => <(input, <(pos, 1) | add) | parse_exponent_tail | (ok & failed)>,
        False => <pos | ok>,
    }
}

proc parse_exponent_tail<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let signed = of (<(<(input, pos) | at, '-') | eq) {
        True => True,
        False => <(<(input, pos) | at, '+') | eq,
    };
    let after_sign = of signed {
        True => <(pos, 1) | add,
        False => pos,
    };
    of (<(input, after_sign) | at | is_digit) {
        True => <(input, after_sign) | parse_digits | ok>,
        False => <"expected digit in exponent" | failed>,
    }
}

proc parse_string<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    <(input, <(pos, 1) | add) | parse_string_tail | (ok & failed)>
}

proc parse_string_tail<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(pos, <input | str_len) | ge) {
        True => <"unterminated JSON string" | failed>,
        False => (,),
    };
    let ch = <(input, pos) | at;
    of (<(ch, QUOTE) | eq) {
        True => <(pos, 1) | add | ok>,
        False => (,),
    };
    of (<(ch, BACKSLASH) | eq) {
        True => <(input, <(pos, 1) | add) | parse_escape | (ok & failed)>,
        False => (,),
    };
    of (<(ch, ' ') | lt) {
        True => <"raw control character in JSON string" | failed>,
        False => (,),
    };
    <(input, <(pos, 1) | add) | parse_string_tail | (ok & failed)>
}

proc parse_escape<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(input, pos) | at) {
        '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => <(input, <(pos, 1) | add)
            | parse_string_tail
            | (ok & failed)>,
        'u' => <(input, <(pos, 1) | add) | parse_hex4 | (ok & failed)>,
        _ => <"invalid escape in JSON string" | failed>,
    }
}

// Whether `count` hexadecimal digits start at `pos`, looking no further than
// the first that is not one.
func hex_digits(input: String, pos: i64, count: i64) -> Bool {
    of (<(count, 0) | eq) {
        True => True,
        False => of (<(input, pos) | at | is_hex) {
            True => <(input, <(pos, 1) | add, <(count, 1) | sub) | hex_digits,
            False => False,
        },
    }
}

proc parse_hex4<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(input, pos, 4) | hex_digits) {
        True => <(input, <(pos, 4) | add) | parse_string_tail | (ok & failed)>,
        False => <"invalid hexadecimal digit in \\u escape" | failed>,
    }
}

proc parse_literal<E>(input: String, pos: i64, text: String) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let end = <(pos, <text | str_len) | add;
    // The slice is taken only once it is known to be in range.
    let matches = of (<(end, <input | str_len) | le) {
        True => <(<(input, pos, end) | substring, text) | eq,
        False => False,
    };
    of matches {
        True => <end | ok>,
        False => <"invalid JSON literal" | failed>,
    }
}

proc parse_array<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let first = <(input, <(pos, 1) | add) | skip_ws;
    of (<(<(input, first) | at, CLOSE_BRACKET) | eq) {
        True => <(first, 1) | add | ok>,
        False => <(input, first) | parse_array_body | (ok & failed)>,
    }
}

proc parse_array_body<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let value_end = mu { k <= <(input, pos) | parse_value | (k & failed)> };
    let after_value = <(input, value_end) | skip_ws;
    let ch = <(input, after_value) | at;
    of (<(ch, CLOSE_BRACKET) | eq) {
        True => <(after_value, 1) | add | ok>,
        False => (,),
    };
    of (<(ch, COMMA) | ne) {
        True => <"expected `,` or `]` in array" | failed>,
        False => (,),
    };
    let next = <(input, <(after_value, 1) | add) | skip_ws;
    of (<(<(input, next) | at, CLOSE_BRACKET) | eq) {
        True => <"trailing comma in array" | failed>,
        False => <(input, next) | parse_array_body | (ok & failed)>,
    }
}

proc parse_object<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    let first = <(input, <(pos, 1) | add) | skip_ws;
    of (<(<(input, first) | at, CLOSE_BRACE) | eq) {
        True => <(first, 1) | add | ok>,
        False => <(input, first) | parse_object_body | (ok & failed)>,
    }
}

proc parse_object_body<E>(input: String, pos: i64) | (
    ok: (-i64 / {..E})
    & failed: (-String / {..E})
) / {..E} {
    of (<(<(input, pos) | at, QUOTE) | ne) {
        True => <"expected object key" | failed>,
        False => (,),
    };
    let key_end = mu { k <= <(input, pos) | parse_string | (k & failed)> };
    let after_key = <(input, key_end) | skip_ws;
    of (<(<(input, after_key) | at, COLON) | ne) {
        True => <"expected `:` after object key" | failed>,
        False => (,),
    };
    let value_pos = <(input, <(after_key, 1) | add) | skip_ws;
    let value_end = mu { k <= <(input, value_pos) | parse_value | (k & failed)> };
    let after_value = <(input, value_end) | skip_ws;
    let ch = <(input, after_value) | at;
    of (<(ch, CLOSE_BRACE) | eq) {
        True => <(after_value, 1) | add | ok>,
        False => (,),
    };
    of (<(ch, COMMA) | ne) {
        True => <"expected `,` or `}` in object" | failed>,
        False => (,),
    };
    let next = <(input, <(after_value, 1) | add) | skip_ws;
    of (<(<(input, next) | at, CLOSE_BRACE) | eq) {
        True => <"trailing comma in object" | failed>,
        False => <(input, next) | parse_object_body | (ok & failed)>,
    }
}

proc main | (exit: i32) / {IO} {
    let source = "{\"name\":\"slant\",\"tags\":[1,2,-3.25],\"active\":true,\"none\":null,\"escaped\":\"a\\\"b\\u0041\"}";

    // One consumer per outcome, each ending in a cut against `exit`.
    let parsed = mu String {
        value => {
            <("parsed: ", value) | add | println;
            <0 | exit>
        },
    };
    let failed = mu String {
        message => {
            <("error: ", message) | add | println;
            <1 | exit>
        },
    };
    <source | parse_json | (parsed & failed)>
}
