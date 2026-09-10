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
// v1 supports tail-resumptive handlers: a clause uses `resume` at most once,
// in tail position. Exceptions (never resuming) are the degenerate case;
// resuming once in tail position covers readers, state-passing, and the like.

// An exception: `throw` never returns to the caller, so its clause does not
// resume — it replaces the computation with the handler's answer.
effect Exn {
    fn throw(message: +String) -> i64;
}

fn checked_div(a: +i64, b: +i64) -> i64 {
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

fn scaled(x: +i64) -> i64 {
    x * config()
}

command main | (exit: -i32) {
    // The exception is caught; the clause ignores `resume`.
    let safe = handle checked_div(10, 0) with Exn {
        throw(message) resume => 0 - 1,
        return(n) => n,
    };
    println(safe);                      // -1

    let ok = handle checked_div(10, 2) with Exn {
        throw(message) resume => 0 - 1,
        return(n) => n,
    };
    println(ok);                        // 5

    // The reader resumes with the configured value.
    let result = handle scaled(7) with Reader {
        config() resume => resume(10),
        return(n) => n,
    };
    println(result);                    // 70

    0 @ exit
}
