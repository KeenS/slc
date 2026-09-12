// `|` is flow: everything moves left to right, and polarity says what
// each step means.
//
//   value    | function   apply       — the value flows in, a result out
//   function | function   compose     — a function awaiting a value
//   function | consumer   compose     — a consumer awaiting a value
//   value    | consumer   cut         — closed at both ends: a command
//
// A chain is flat because composition is associative, and a consumer may
// stand only at the right end: nothing flows out of one.

fn double(n: i64) -> i64 { n * 2 }
fn incr(n: i64) -> i64 { n + 1 }

// A two-exit command, written unary: one value, one menu of exits, each
// component naming what reaches it.
command classify(n: i64) | (found: i64 & missing: String) {
    if n > 0 { n | found } else { "nothing there" | missing }
}

// A row is a value: this one takes the whole menu and hands it on.
command forward(n: i64) | (row: (-i64 & -String)) {
    classify(n, row)
}

command main | (exit: -i32) {
    // apply, then a chain of applications
    println(21 | double);
    println(3 | double | incr | double);

    // the cut — the same expression however the chain is split, because
    // composition is associative
    println(mu i64 { out <= 21 | double | out });
    println(mu i64 { out <= 21 | (double | out) });

    // `double | incr | out` is a consumer, awaiting a value
    println(mu i64 { out <= 5 | (double | incr | out) });

    // a two-exit command: its exits spread, then bundled
    println(mu i64 { ok <= classify(7, ok, select +String { s => str_len(s) | ok }) });
    println(mu i64 { ok <= classify(0 - 1, (ok & select +String { s => str_len(s) | ok })) });
    println(mu i64 { ok <= forward(0 - 1, (ok & select +String { s => str_len(s) | ok })) });

    0 | exit
}
