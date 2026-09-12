// `|` is flow: everything moves left to right, and every step composes.
// Brackets say where a chain is closed — and the stage beside a bracket
// takes its role from it:
//
//   v | f | k⟩    closed at both ends: a value in, a consumer at the end
//                  — a command, and the only thing that is
//   v | f         closed at the left: a value flowing on, awaiting a
//                  continuation
//   f | k⟩         closed at the right: a consumer, awaiting a value
//   f | g          neither: function composition, always
//
// So `f | k` never has to be read twice: unbracketed it composes, and the
// cut that sends `f` itself to `k` is `f | k⟩`.

fn double(n: i64) -> i64 { n * 2 }
fn incr(n: i64) -> i64 { n + 1 }

// Closed at the right only: a consumer, awaiting a value.
fn doubling(k: -i64) -> -i64 {
    double | incr | k⟩
}

// A two-exit command, written unary: one value, one menu of exits, each
// component naming what reaches it.
command classify(n: i64) | (found: i64 & missing: String) {
    if n > 0 { n | found⟩ } else { "nothing there" | missing⟩ }
}

// A row is a value: this one takes the whole menu and hands it on.
command forward(n: i64) | (row: (-i64 & -String)) {
    classify(n, row)
}

command main | (exit: -i32) {
    // a value flowing through functions, awaiting a continuation
    21 | double | println;
    3 | double | incr | double | println;

    // the cut — closed at both ends
    (mu i64 { out <= 21 | double | out⟩ } | println);

    // the same chain, split: `double | k⟩` is a consumer on its own, so
    // feeding it is the same command
    (mu i64 { out <= 5 | double | incr | out⟩ } | println);
    (mu i64 { out <= 5 | (out | doubling)⟩ } | println);

    // plain composition: two functions make a function
    let quadruple = double | double;
    5 | quadruple | println;

    // a two-exit command: its exits spread, then bundled, then forwarded
    (mu i64 { ok <= classify(7, ok, select +String { s => s | str_len | ok⟩ }) } | println);
    (mu i64 { ok <= classify(0 - 1, (ok & select +String { s => s | str_len | ok⟩ })) } | println);
    (mu i64 { ok <= forward(0 - 1, (ok & select +String { s => s | str_len | ok⟩ })) } | println);

    0 | exit⟩
}
