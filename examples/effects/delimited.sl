// Delimited control: a handler delimits `mu`, and `reset` delimits without
// handling.
//
// `mu` captures the rest of the program, but a jump to it replaces the running
// stack only down to the nearest handler the jump and the capture share — the
// handler it was captured under, or the copy of it a `resume` reinstated. So a
// clause that resumes twice gets both answers back, even when the resumed code
// jumps to a continuation captured before it performed. A jump from under a
// handler the continuation was not captured under is refused; see
// `delimited_error.sl`.

hook Choose { func flip() -> Bool; }
hook Reader { func config() -> i64; }
hook Judge { func judge(n: i64, ok: -String, bad: -String) -> (;); }

// `r` is captured before `flip` is performed, so each resumption jumps to a
// continuation captured outside it, and lands in its own copy of the handler.
func pick() -> String / {Choose} {
    let a = mu String { r <= <(of flip() { True => "H", False => "T" }) | r> };
    a
}

// The same with a `reset` between the handler and the operation: each
// resumption carries a copy of the `reset` too, and the jump lands on it.
func pick_under_reset() -> String / {Choose} {
    reset {
        let a = mu String { r <= <(of flip() { True => "H", False => "T" }) | r> };
        a
    }
}

// An early exit from handled code: `out` is captured under the handler, so
// the jump made after `config` resumed stays under it.
func check(out: -String) -> String / {Reader} {
    of (<(config(), 0) | lt) { True => <"negative, stopping early" | out>, False => "fine" }
}

func read_twice() -> i64 / {Reader} { <(config(), config()) | add }

proc main | (exit: i32) / {IO} {
    let all = do pick() {
        flip(): resume => <(<True | resume, " ") | add | x => (x, <False | resume) | add,
    };
    <all | println; // H T

    let crossed = do pick_under_reset() {
        flip(): resume => <(<True | resume, " ") | add | x => (x, <False | resume) | add,
    };
    <crossed | println; // H T

    // A clause cuts into the continuations it is handed: it runs below its
    // handler, on frames they share.
    let verdict = do (mu String { k <= <(5, k, k) | judge> }) {
        judge(n, ok, bad) => of (<(n, 3) | gt) {
            True => <"big" | ok>,
            False => <"small" | bad>,
        },
    };
    <verdict | println; // big

    let stopped = do (mu String { out <= <(<out | check) | out> }) {
        config(): resume => <-1 | resume,
    };
    <stopped | println; // negative, stopping early

    // What `reset` does not handle passes through to the handler around it.
    let n = do (reset read_twice()) {
        config(): resume => <21 | resume,
    };
    <n | println; // 42

    <0 | exit>
}
