// Append N one-character pieces, alternating "a" and "b", and sum their
// scalar values. Even n appends "a" (97) and odd n appends "b" (98).

def N: i64 = 5000;

func grow(b: string::Builder, n: i64) -> string::Builder {
    of (<(n, 0) | eq) {
        True => b,
        _ => {
            let piece = of (<(<(n, 2) | rem, 0) | eq) {
                True => "a",
                _ => "b",
            };
            let next = <(b, piece) | string::push;
            <(next, <(n, 1) | sub) | grow
        },
    }
}

func codes(s: String, i: i64, acc: i64) -> i64 {
    of (<(i, <s | str_len) | lt) {
        True => {
            let code = <(<(s, i) | index) | char_to_code;
            <(s, <(i, 1) | add, <(acc, code) | add) | codes
        },
        False => acc,
    }
}

proc main | (exit: i32) / {IO} {
    let built = <(string::new(), N) | grow;
    <(built.finish, 0, 0) | codes | println;
    <0 | exit>
}
