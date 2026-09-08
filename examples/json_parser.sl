// JSON parser demonstrating error handling by continuations.
//
// The parser is written in direct style: at each step it either advances
// or fails. On failure, it escapes to the error continuation with a
// message; there is no Result enum and no pattern matching on the way out.
//
// Structure (simplified for the current runtime):
//   parse_json(input) returns either a parsed value or escapes.

fn parse_value(input: String, pos: i64) -> String {
    // Dispatch on the character at `pos`.
    // Numbers: parse digits directly.
    // Strings: parse quoted strings.
    // Arrays/objects: simplified — reject for now.
    let ch = char_at(input, pos);

    if is_digit(ch) {
        int_to_str(parse_number(input, pos))
    } else {
        if eq(ch, 34) {
            // 34 is the char code for `"`
            parse_string(input, add(pos, 1))
        } else {
            mu(escape: -i32) {
                escape(-1)
            }
        }
    }
}

fn parse_number(input: String, pos: i64) -> i64 {
    // Read digits starting at pos; return the number.
    let start = pos;
    let end = skip_digits(input, pos);
    str_to_int(substring(input, start, end))
}

fn parse_string(input: String, pos: i64) -> String {
    // Starting after the opening quote, find the closing quote.
    // Return the substring between them.
    let end = find_char(input, pos, 34);
    substring(input, pos, end)
}

fn parse_json(input: String) -> String {
    // Entry point: skip whitespace, parse a value, expect end of input.
    let start = skip_ws(input, 0);
    parse_value(input, start)
}

fn main() -> i32 {
    // Valid JSON number
    println(parse_json("42"));

    // Valid JSON string
    println(parse_json("\"hello\""));

    // Invalid input: `null` is not yet supported, so it escapes to -1.
    println(parse_json("null"))
}
