// An exception that replaces the computation, and a reader that resumes.

hook Exn { func throw(message: String) -> i64; }
hook Reader { func config() -> i64; }

func checked_div(a: i64, b: i64) -> i64 / {Exn} {
    of (<(b, 0) | eq) {
        True => <"division by zero" | throw,
        False => <(a, b) | div,
    }
}

func scaled(x: i64) -> i64 / {Reader} {
    <(x, config()) | mul
}

proc main | (exit: i32) / {IO} {
    let safe = do (<(10, 0) | checked_div) hn { throw(m) => -1 };
    <safe | println;
    let ok = do (<(10, 2) | checked_div) hn { throw(m) => -1 };
    <ok | println;
    let reading = do (<7 | scaled) hn { config(): resume => <10 | resume };
    <reading | println;
    <0 | exit>
}
