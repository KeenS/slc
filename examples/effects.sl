// Algebraic effects: a computation performs operations, a handler answers.
//
// An `effect` names operations; performing one suspends the computation and
// hands control to the nearest `handle`, which answers with a clause. The
// clause receives `resume`, the captured continuation — a first-class value.
// A clause may resume any number of times: not at all is an exception, once
// is a reader or state, and twice is nondeterminism.
//
// An operation is a free function (like a trait method); a handler is the
// dual of a trait — a trait hands a value the functions it provides, an
// effect hands a computation the answers it demands.

effect Exn { fn throw(message: +String) -> i64; }
effect Reader { fn config() -> i64; }
effect Choose { fn flip() -> bool; }

// An exception: `throw` never returns, so its clause does not resume.
fn checked_div(a: +i64, b: +i64) -> i64 / {Exn} {
    if b == 0 { throw("division by zero") } else { a / b }
}

// A reader: `config` asks the handler and continues — one resume, and work
// after it composes.
fn scaled(x: +i64) -> i64 / {Reader} {
    x * config()
}

// Nondeterminism: two choices, and the handler takes both by resuming twice.
fn pick() -> String / {Choose} {
    let a = if flip() { "H" } else { "T" };
    let b = if flip() { "H" } else { "T" };
    a + b
}

// Row polymorphism, written the way generics are: a row variable is a
// generic parameter, used with the `..` "rest" spelling. The prelude's map
// says exactly what it forwards —
//
//   fn map<A, B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
//
// — so `map(half, xs)` instantiates E to half's row `{Exn}`, and the
// handler around the call is what keeps `main` pure.
fn half(n: +i64) -> i64 / {Exn} {
    if n % 2 == 0 { n / 2 } else { throw("odd") }
}

// A negative function carries its row in the same place — after the `<-`
// arrow — and it means the same thing: performed on the function's watch.
fn emit(out: -i64) <- i64 / {Reader} {
    fn(x: +i64) { x * config() | out⟩ }
}

command main | (exit: -i32) {
    // never resumes — the exception replaces the computation
    let safe = handle checked_div(10, 0) { throw(m) => 0 - 1, return(n) => n };
    println(safe);                       // -1

    let ok = handle checked_div(10, 2) { throw(m) => 0 - 1, return(n) => n };
    println(ok);                         // 5

    // resumes once, then does work after the resume
    let r = handle scaled(7) { config(): resume => resume(10) + 1000, return(n) => n };
    println(r);                          // 7*10 + 1000 = 1070

    // resumes twice, combining both branches of every choice
    let all = handle pick() {
        flip(): resume => resume(true) + " " + resume(false),
        return(s) => s,
    };
    println(all);                        // "HH HT TH TT"

    // the row of `map(half, …)` is `half`'s row, forwarded — handled here
    let halves = handle map(half, Cons(8, Cons(4, Nil))) {
        throw(m) => Nil,
        return(xs) => xs,
    };
    println(fmt(halves));                // "[4, 2]"
    let none = handle map(half, Cons(8, Cons(5, Nil))) {
        throw(m) => Nil,
        return(xs) => xs,
    };
    println(fmt(none));                  // "[]"

    // the negative function's row, discharged like any other
    println(handle (mu i64 { out <= 6 | emit(out)⟩ }) {
        config(): resume => resume(7),
        return(n) => n,
    });                                  // 42

    0 | exit⟩
}
