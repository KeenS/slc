// Algebraic effects: a computation performs operations, a handler answers.
//
// An `effect` names operations; performing one suspends the computation and
// hands control to the nearest `handle`, which answers with a clause. The
// clause receives `resume`, the captured continuation: calling it continues
// the computation with a result, not calling it abandons it.
//
// A handler is installed dynamically, and an operation is a free function
// (like a trait method) — the dual of a trait: a trait hands a value the
// functions it provides, an effect hands a computation the answers it demands.
//
// v1 supports single-shot handlers: a clause resumes at most once per path,
// in any position. Not resuming at all is an exception; resuming once — even
// with work after it — covers readers, state, logging, and the like.
// Resuming twice (full nondeterminism) is not yet supported.

// An exception: `throw` never returns to the caller, so its clause does not
// resume — it replaces the computation with the handler's answer.
effect Exn {
    fn throw(message: +String) -> i64;
}

fn checked_div(a: +i64, b: +i64) -> i64 / {Exn} {
    if b == 0 {
        throw("division by zero")
    } else {
        a / b
    }
}

// A reader: `config` asks the handler for a value and continues with it —
// one tail resume.
effect Reader {
    fn config() -> i64;
}

fn scaled(x: +i64) -> i64 / {Reader} {
    x * config()
}

command main | (exit: -i32) {
    // The exception is caught; the clause ignores `resume`.
    let safe = handle checked_div(10, 0) {
        throw(message) resume => 0 - 1,
        return(n) => n,
    };
    println(safe);                      // -1

    let ok = handle checked_div(10, 2) {
        throw(message) resume => 0 - 1,
        return(n) => n,
    };
    println(ok);                        // 5

    // The reader resumes with the configured value.
    // `resume(10)` continues `scaled` with config = 10, yielding 70; the
    // clause then adds 1000 — work after a resume, which single-shot allows.
    let result = handle scaled(7) {
        config() resume => resume(10) + 1000,
        return(n) => n,
    };
    println(result);                    // 70 + 1000 = 1070

    0 @ exit
}
