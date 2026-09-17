// The dual of effects: latent rows on the negative side.
//
// A function's row fires at application — the work runs before the value
// exists. Codata is the mirror: a menu answers per demand, a form runs
// when fed, so their work runs AFTER the value exists, on the consumer's
// schedule. A latent row on the declaration says what that work may
// perform; every demand incurs it, and the handler that discharges it is
// the one around the demand — not the one around the construction.

effect Exn { fn throw(message: String) -> i64; }

// The row belongs to the type: demanding a Fallible may throw.
menu Fallible / {Exn} {
    value: i64,
    doubled: i64,
}

// The constructor declares no row — it performs nothing. Its arms belong
// to Fallible's latent row and are checked against it.
fn checked(n: i64) -> Fallible {
    mu Fallible {
        value <= <(match (<(n, 0) | ge) { True => n, False => <"negative" | throw }) | value>,
        doubled <= <(match (<(n, 0) | ge) {
            True => <(n, 2) | mul,
            False => <"negative" | throw,
        }) | doubled>,
    }
}

fn risky(x: i64) -> i64 / {Exn} {
    <"late" | throw
}

effect Reader { fn config() -> i64; }

// Codata that reads its configuration per demand. (One item, so the
// canonical copattern spells the binder out — a single pun arm would read
// as the binder form of `mu`.)
menu Scaled / {Reader} {
    amount: i64,
}

fn scaled(base: i64) -> Scaled {
    mu Scaled {
        amount: out <= <(base, config()) | mul | out>,
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
        Validated { age, out } => <(match (<(age, 18) | ge) {
            True => age,
            False => <"too young" | throw,
        }) | out>,
    }
}

command main | (exit: i32) / {IO} {
    let f = <21 | checked;
    // The handler wraps the DEMAND — the honest extent. The same value can
    // answer under different handlers, one per demand.
    <handle f.value { throw(m) => -1 } | println;
    <handle f.doubled { throw(m) => -1 } | println;
    <handle (<-5 | checked).value { throw(m) => -1 } | println;

    // A consumer carries a latent row too: `risky | out` composes without
    // performing anything — the row fires when the consumer is FED, so the
    // handler belongs around the cut.
    let n = handle (mu i64 { out <= <5 | risky | out> }) {
        throw(m) => -1,
    };
    <n | println;

    // What no trait can do: the SAME value answers the SAME demand
    // differently under different handlers — dispatch is chosen per
    // demand by the dynamic context, not sealed into the type.
    let s = <7 | scaled;
    <handle s.amount { config(): resume => <10 | resume } | println;
    <handle s.amount { config(): resume => <100 | resume } | println;

    // Feeding a rowed form under a handler: the arm's throw fires at the
    // feed, in this extent, and lands in this handler.
    <handle (mu i64 { k <= <Validated { age: 21, out: k } | admit()> }) {
        throw(m) => -1,
    } | println;
    <handle (mu i64 { k <= <Validated { age: 15, out: k } | admit()> }) {
        throw(m) => -1,
    } | println;
    <0 | exit>
}
