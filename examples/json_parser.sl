// A continuation-based JSON parser.
//
// This example deliberately avoids a Result enum. Parsing receives two
// continuations:
//
//   * `ok` receives the parsed JSON text
//   * `err` receives a diagnostic message
//
// The error continuation escapes through `ret`, so a failure never falls
// through to a success continuation. The parser validates the complete
// input, including trailing characters and trailing commas.
//
// v0.1 runtime notes:
//   * JSON values are represented by their validated source slice.
//   * Binary operators are not lowered yet, so comparisons use builtins.

fn parse_json(
    input: +String,
    ok: +String,
    err: +String,
) -> i64 {
    let start = skip_ws(input, 0);
    if lt(start, str_len(input)) {
        let end = parse_value(input, start, ok, err);
        let stopped = skip_ws(input, end);
        if eq(stopped, str_len(input)) {
            ok(substring(input, start, end))
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
    let ch = char_at(input, pos);
    if is_digit(ch) {
        parse_number(input, pos, ok, err)
    } else {
        if eq(ch, 45) {
            parse_number(input, pos, ok, err)
        } else {
            if eq(ch, 34) {
                parse_string(input, pos, ok, err)
            } else {
                if eq(ch, 91) {
                    parse_array(input, pos, ok, err)
                } else {
                    if eq(ch, 123) {
                        parse_object(input, pos, ok, err)
                    } else {
                        if eq(ch, 116) {
                            parse_literal(input, pos, "true", ok, err)
                        } else {
                            if eq(ch, 102) {
                                parse_literal(input, pos, "false", ok, err)
                            } else {
                                if eq(ch, 110) {
                                    parse_literal(input, pos, "null", ok, err)
                                } else {
                                    err("expected JSON value")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn parse_number(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let after_sign = if eq(char_at(input, pos), 45) {
        add(pos, 1)
    } else {
        pos
    };
    let integer_end = parse_digits(input, after_sign);
    if gt(integer_end, after_sign) {
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
    let after_fraction = if eq(char_at(input, pos), 46) {
        parse_fraction(input, add(pos, 1), ok, err)
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
    if is_digit(char_at(input, pos)) {
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
    let ch = char_at(input, pos);
    if eq(ch, 101) {
        parse_exponent_tail(input, add(pos, 1), ok, err)
    } else {
        if eq(ch, 69) {
            parse_exponent_tail(input, add(pos, 1), ok, err)
        } else {
            pos
        }
    }
}

fn parse_exponent_tail(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let after_sign = if eq(char_at(input, pos), 45) {
        add(pos, 1)
    } else {
        if eq(char_at(input, pos), 43) {
            add(pos, 1)
        } else {
            pos
        }
    };
    if is_digit(char_at(input, after_sign)) {
        parse_digits(input, after_sign)
    } else {
        err("expected digit in exponent")
    }
}

fn parse_digits(input: +String, pos: +i64) -> i64 {
    if lt(pos, str_len(input)) {
        if is_digit(char_at(input, pos)) {
            parse_digits(input, add(pos, 1))
        } else {
            pos
        }
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
    parse_string_tail(input, add(pos, 1), ok, err)
}

fn parse_string_tail(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    if ge(pos, str_len(input)) {
        err("unterminated JSON string")
    } else {
        let ch = char_at(input, pos);
        if eq(ch, 34) {
            add(pos, 1)
        } else {
            if eq(ch, 92) {
                parse_escape(input, add(pos, 1), ok, err)
            } else {
                if lt(ch, 32) {
                    err("raw control character in JSON string")
                } else {
                    parse_string_tail(input, add(pos, 1), ok, err)
                }
            }
        }
    }
}

fn parse_escape(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let ch = char_at(input, pos);
    if eq(ch, 34) {
        parse_string_tail(input, add(pos, 1), ok, err)
    } else {
        if eq(ch, 92) {
            parse_string_tail(input, add(pos, 1), ok, err)
        } else {
            if eq(ch, 47) {
                parse_string_tail(input, add(pos, 1), ok, err)
            } else {
                if eq(ch, 98) {
                    parse_string_tail(input, add(pos, 1), ok, err)
                } else {
                    if eq(ch, 102) {
                        parse_string_tail(input, add(pos, 1), ok, err)
                    } else {
                        if eq(ch, 110) {
                            parse_string_tail(input, add(pos, 1), ok, err)
                        } else {
                            if eq(ch, 114) {
                                parse_string_tail(input, add(pos, 1), ok, err)
                            } else {
                                if eq(ch, 116) {
                                    parse_string_tail(input, add(pos, 1), ok, err)
                                } else {
                                    if eq(ch, 117) {
                                        parse_hex4(input, add(pos, 1), ok, err)
                                    } else {
                                        err("invalid escape in JSON string")
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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
    let ch = char_at(input, pos);
    if is_digit(ch) {
        add(pos, 1)
    } else {
        if ge(ch, 97) {
            if le(ch, 102) {
                add(pos, 1)
            } else {
                err("invalid hexadecimal digit in \\u escape")
            }
        } else {
            if ge(ch, 65) {
                if le(ch, 70) {
                    add(pos, 1)
                } else {
                    err("invalid hexadecimal digit in \\u escape")
                }
            } else {
                err("invalid hexadecimal digit in \\u escape")
            }
        }
    }
}

fn parse_literal(
    input: +String,
    pos: +i64,
    text: +String,
    ok: +String,
    err: +String,
) -> i64 {
    let end = add(pos, str_len(text));
    if str_eq(substring(input, pos, end), text) {
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
    let first = skip_ws(input, add(pos, 1));
    if eq(char_at(input, first), 93) {
        add(first, 1)
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
    let ch = char_at(input, after_value);
    if eq(ch, 44) {
        let next = skip_ws(input, add(after_value, 1));
        if eq(char_at(input, next), 93) {
            err("trailing comma in array")
        } else {
            parse_array_body(input, next, ok, err)
        }
    } else {
        if eq(ch, 93) {
            add(after_value, 1)
        } else {
            err("expected `,` or `]` in array")
        }
    }
}

fn parse_object(
    input: +String,
    pos: +i64,
    ok: +String,
    err: +String,
) -> i64 {
    let first = skip_ws(input, add(pos, 1));
    if eq(char_at(input, first), 125) {
        add(first, 1)
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
    if eq(char_at(input, pos), 34) {
        let key_end = parse_string(input, pos, ok, err);
        let after_key = skip_ws(input, key_end);
        if eq(char_at(input, after_key), 58) {
            let value_start = skip_ws(input, add(after_key, 1));
            let value_end = parse_value(input, value_start, ok, err);
            let after_value = skip_ws(input, value_end);
            let ch = char_at(input, after_value);
            if eq(ch, 44) {
                let next = skip_ws(input, add(after_value, 1));
                if eq(char_at(input, next), 125) {
                    err("trailing comma in object")
                } else {
                    parse_object_body(input, next, ok, err)
                }
            } else {
                if eq(ch, 125) {
                    add(after_value, 1)
                } else {
                    err("expected `,` or `}` in object")
                }
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
            println(str_concat("parsed: ", value));
            ret(0)
        };
        let err = fn(message: +String) -> i32 {
            println(str_concat("error: ", message));
            ret(1)
        };
        parse_json(source, ok, err)
    }
}
