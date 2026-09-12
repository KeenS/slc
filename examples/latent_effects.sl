// The dual of effects: latent rows on the negative side.
//
// A function's row fires at application — the work runs before the value
// exists. Codata is the mirror: a menu answers per demand, a form runs
// when fed, so their work runs AFTER the value exists, on the consumer's
// schedule. A latent row on the declaration says what that work may
// perform; every demand incurs it, and the handler that discharges it is
// the one around the demand — not the one around the construction.

effect Exn { fn throw(message: +String) -> i64; }

// The row belongs to the type: demanding a Fallible may throw.
menu Fallible / {Exn} {
    value: i64,
    doubled: i64,
}

// The constructor declares no row — it performs nothing. Its arms belong
// to Fallible's latent row and are checked against it.
fn checked(n: +i64) -> Fallible {
    mu Fallible {
        value <= (if n >= 0 { n } else { throw("negative") }) | value⟩,
        doubled <= (if n >= 0 { n * 2 } else { throw("negative") }) | doubled⟩,
    }
}

fn risky(x: +i64) -> i64 / {Exn} {
    throw("late")
}

effect Reader { fn config() -> i64; }

// Codata that reads its configuration per demand. (One item, so the
// canonical copattern spells the binder out — a single pun arm would read
// as the binder form of `mu`.)
menu Scaled / {Reader} {
    amount: i64,
}

fn scaled(base: +i64) -> Scaled {
    mu Scaled {
        amount: out <= base * config() | out⟩,
    }
}

// A rowed form: feeding it may throw. The record carries the continuation
// the answer flows to; the throw escapes to whoever wraps the feed.
form Validated / {Exn} {
    age: i64,
    out: -i64,
}

fn admit() -> Validated {
    select Validated {
        Validated { age, out } => (if age >= 18 { age } else { throw("too young") }) | out⟩,
    }
}

command main | (exit: -i32) {
    let f = checked(21);
    // The handler wraps the DEMAND — the honest extent. The same value can
    // answer under different handlers, one per demand.
    println(handle f.value { throw(m) => 0 - 1, return(n) => n });
    println(handle f.doubled { throw(m) => 0 - 1, return(n) => n });
    println(handle checked(0 - 5).value { throw(m) => 0 - 1, return(n) => n });

    // A consumer carries a latent row too: `risky | out` composes without
    // performing anything — the row fires when the consumer is FED, so the
    // handler belongs around the cut.
    let n = handle (mu i64 { out <= 5 | risky | out⟩ }) {
        throw(m) => 0 - 1,
        return(x) => x,
    };
    println(n);

    // What no trait can do: the SAME value answers the SAME demand
    // differently under different handlers — dispatch is chosen per
    // demand by the dynamic context, not sealed into the type.
    let s = scaled(7);
    println(handle s.amount { config(): resume => resume(10), return(n) => n });
    println(handle s.amount { config(): resume => resume(100), return(n) => n });

    // Feeding a rowed form under a handler: the arm's throw fires at the
    // feed, in this extent, and lands in this handler.
    println(handle (mu i64 { k <= Validated { age: 21, out: k } | admit()⟩ }) {
        throw(m) => 0 - 1,
        return(n) => n,
    });
    println(handle (mu i64 { k <= Validated { age: 15, out: k } | admit()⟩ }) {
        throw(m) => 0 - 1,
        return(n) => n,
    });
    0 | exit⟩
}
