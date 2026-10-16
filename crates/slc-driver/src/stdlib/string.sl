// `string`: a persistent string builder, and scans over the text itself.
//
// The builder is a menu. The scans are ordinary functions over the prelude's
// string primitives: a prefix test, the scalar values in order, splitting,
// trimming, and replacement. Case mapping is `__lower` and `__upper`, because
// a Unicode case table is not something the language can write down.

cite list::List;
cite list::List::*;

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

// True when `text` begins with `prefix`, including the empty prefix.
pub func starts_with(text: String, prefix: String) -> Bool {
    let n = <prefix | str_len;
    let m = <text | str_len;
    of (<(n, m) | gt) {
        True => False,
        False => <(<(text, 0, n) | substring, prefix) | eq,
    }
}

func chars_from(text: String, i: i64, n: i64) -> List<char> {
    of (<(i, n) | ge) {
        True => Nil,
        False => Cons(<(text, i) | index, <(text, <(i, 1) | add, n) | chars_from),
    }
}

// The scalar values, in order. An empty string is `Nil`.
pub func chars(text: String) -> List<char> {
    <(text, 0, <text | str_len) | chars_from
}

func at_sep(text: String, sep: String, i: i64) -> Bool {
    let m = <sep | str_len;
    <(<(text, i, <(i, m) | add) | substring, sep) | eq
}

// The index of `sep` at or after `i`, or `-1` when it does not occur.
func find_sep(text: String, sep: String, i: i64) -> i64 {
    let last = <(<text | str_len, <sep | str_len) | sub;
    of (<(i, last) | gt) {
        True => -1,
        False => of (<(text, sep, i) | at_sep) {
            True => i,
            False => <(text, sep, <(i, 1) | add) | find_sep,
        },
    }
}

func split_chars(text: String, i: i64, n: i64) -> List<String> {
    of (<(i, n) | ge) {
        True => Nil,
        False => Cons(
            <(text, i, <(i, 1) | add) | substring,
            <(text, <(i, 1) | add, n) | split_chars,
        ),
    }
}

func split_from(text: String, sep: String, i: i64) -> List<String> {
    let at = <(text, sep, i) | find_sep;
    of (<(at, 0) | lt) {
        True => Cons(<(text, i, <text | str_len) | substring, Nil),
        False => Cons(
            <(text, i, at) | substring,
            <(text, sep, <(at, <sep | str_len) | add) | split_from,
        ),
    }
}

// Split on `sep`. An empty separator yields one string per scalar value, and
// an empty string then yields `Nil`. A separator that is absent yields the
// one-element list of `text`, and a trailing separator yields a trailing
// empty string.
pub func split(text: String, sep: String) -> List<String> {
    of (<(<sep | str_len, 0) | eq) {
        True => <(text, 0, <text | str_len) | split_chars,
        False => <(text, sep, 0) | split_from,
    }
}

func skip_leading(text: String, i: i64, n: i64) -> i64 {
    of (<(i, n) | ge) {
        True => i,
        False => of (<(text, i) | index | is_ws) {
            True => <(text, <(i, 1) | add, n) | skip_leading,
            False => i,
        },
    }
}

func skip_trailing(text: String, i: i64) -> i64 {
    of (<(i, 0) | le) {
        True => 0,
        False => of (<(text, <(i, 1) | sub) | index | is_ws) {
            True => <(text, <(i, 1) | sub) | skip_trailing,
            False => i,
        },
    }
}

// Drop leading and trailing whitespace. The middle is kept, including its
// own whitespace.
pub func trim(text: String) -> String {
    let n = <text | str_len;
    let start = <(text, 0, n) | skip_leading;
    let end = <(text, n) | skip_trailing;
    of (<(start, end) | ge) {
        True => "",
        False => <(text, start, end) | substring,
    }
}

func replace_from(text: String, from: String, to: String, i: i64) -> String {
    let at = <(text, from, i) | find_sep;
    of (<(at, 0) | lt) {
        True => <(text, i, <text | str_len) | substring,
        False => {
            let before = <(text, i, at) | substring;
            let rest = <(text, from, to, <(at, <from | str_len) | add) | replace_from;
            <(<(before, to) | add, rest) | add
        },
    }
}

// Replace every non-overlapping occurrence of `from`. An empty `from` is not
// a place to split, so the text is unchanged.
pub func replace(text: String, from: String, to: String) -> String {
    of (<(<from | str_len, 0) | eq) {
        True => text,
        False => <(text, from, to, 0) | replace_from,
    }
}

pub func lower(text: String) -> String { <text | __lower }

pub func upper(text: String) -> String { <text | __upper }
