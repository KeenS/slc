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
        value <= (if n >= 0 { n } else { throw("negative") }) @ value,
        doubled <= (if n >= 0 { n * 2 } else { throw("negative") }) @ doubled,
    }
}

fn risky(x: +i64) -> i64 / {Exn} {
    throw("late")
}

command main | (exit: -i32) {
    let f = checked(21);
    // The handler wraps the DEMAND — the honest extent. The same value can
    // answer under different handlers, one per demand.
    println(handle f.value { throw(m) resume => 0 - 1, return(n) => n });
    println(handle f.doubled { throw(m) resume => 0 - 1, return(n) => n });
    println(handle checked(0 - 5).value { throw(m) resume => 0 - 1, return(n) => n });

    // A consumer carries a latent row too: `then(risky, out)` performs
    // nothing when called — `(-A / {..E})` says the row fires when the
    // consumer is FED, so the handler belongs around the cut.
    let n = handle (mu i64 { out <= 5 @ then(risky, out) }) {
        throw(m) resume => 0 - 1,
        return(x) => x,
    };
    println(n);
    0 @ exit
}
