// Algebraic effects: a computation performs operations, a handler answers.
//
// An `hook` names operations; performing one suspends the computation and
// hands control to the nearest `do`, which answers with a clause. The
// clause receives `resume`, the captured continuation — a first-class value.
// A clause may resume any number of times: not at all is an exception, once
// is a reader or state, and twice is nondeterminism.
//
// An operation is a free function (like a trait method); a handler is the
// dual of a trait — a trait hands a value the functions it provides, an
// effect hands a computation the answers it demands.

cite list::List;
cite list::List::*;
cite list::map;

hook Exn { func throw(message: String) -> i64; }
hook Reader { func config() -> i64; }
hook Choose { func flip() -> Bool; }

// An exception: `throw` never returns, so its clause does not resume.
func checked_div(a: i64, b: i64) -> i64 / {Exn} {
    of (<(b, 0) | eq) { True => <"division by zero" | throw, False => <(a, b) | div }
}

// A reader: `config` asks the handler and continues — one resume, and work
// after it composes.
func scaled(x: i64) -> i64 / {Reader} {
    <(x, config()) | mul
}

// Nondeterminism: two choices, and the handler takes both by resuming twice.
func pick() -> String / {Choose} {
    let a = of flip() { True => "H", False => "T" };
    let b = of flip() { True => "H", False => "T" };
    <(a, b) | add
}

// Row polymorphism, written the way generics are: a row variable is a
// generic parameter, used with the `..` "rest" spelling. The prelude's map
// says exactly what it forwards —
//
//   fn map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
//
// — so `map(half, xs)` instantiates E to half's row `{Exn}`, and the
// handler around the call is what keeps `main` pure.
func half(n: i64) -> i64 / {Exn} {
    of (<(n, 2) | rem | x => (x, 0) | eq) { True => <(n, 2) | div, False => <"odd" | throw }
}

// A negative function carries its row in the same place — after the `<-`
// arrow — and it means the same thing: performed on the function's watch.
func emit(out: i64) <- i64 / {Reader} {
    fn(x: i64) { <(x, config()) | mul | out> }
}

proc main | (exit: i32) / {IO} {
    // never resumes — the exception replaces the computation
    let safe = do (<(10, 0) | checked_div) hn { throw(m) => -1 };
    <safe | println; // -1

    let ok = do (<(10, 2) | checked_div) hn { throw(m) => -1 };
    <ok | println; // 5

    // resumes once, then does work after the resume
    let r = do (<7 | scaled) hn { config(): resume => <(<10 | resume, 1000) | add };
    <r | println; // 7*10 + 1000 = 1070

    // resumes twice, combining both branches of every choice
    let all = do pick() hn {
        flip(): resume => <(<True | resume, " ") | add | x => (x, <False | resume) | add,
    };
    <all | println; // "HH HT TH TT"

    // the row of `map(half, …)` is `half`'s row, forwarded — handled here
    let halves = do (<(half, Cons(8, Cons(4, Nil))) | map) hn {
        throw(m) => Nil,
    };
    <halves | fmt | println; // "[4, 2]"
    let none = do (<(half, Cons(8, Cons(5, Nil))) | map) hn {
        throw(m) => Nil,
    };
    <none | fmt | println; // "[]"

    // the negative function's row, discharged like any other
    <do (<6 | emit) hn {
        config(): resume => <7 | resume,
    } | println; // 42

    <0 | exit>
}
