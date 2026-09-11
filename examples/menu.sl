// `menu` — the negative additive, the mirror of `enum`.
//
// An enum value is one tagged variant the producer chose; a menu value
// answers one item the consumer demands. The same two operations work on
// both, and each keyword keeps its role across the mirror:
//
//   `select` answers *data*: it builds the consumer of a positive type,
//   one arm per shape that can arrive. `mu` answers *demands*: with arms,
//   it builds a menu, one arm per request that can arrive — the ambient
//   consumer chooses the branch, and only the demanded one ever runs.
//
//   `match` — a branch table applied to a named scrutinee, on either side.
//   Over an enum value it takes data apart; over a continuation of a menu
//   type it takes the *request* apart — `.item(out)` binds the continuation
//   the request carries, and each arm can answer it or build another.

menu Config {
    retries: i32,
    name: String,
}

// Build a menu: one arm per item. `out` is the continuation the request
// carries; the arm answers by cutting into it.
fn config() -> Config {
    mu Config {
        .retries(out) <= 3 @ out,
        .name(out) <= "slant" @ out,
    }
}

// A menu built from another menu: answer `name` differently, forward the
// rest. Only the demanded item is ever computed.
fn loud(base: Config) -> Config {
    mu Config {
        .retries(out) <= base.retries @ out,
        .name(out) <= base.name + "!" @ out,
    }
}

// Match on a continuation: the scrutinee is a request, `.item(out)` is its
// shape, and each arm returns another request — `.item(k)` is a request
// literal, the mirror of an enum variant expression.
fn reroute(k: -Config) -> -Config {
    match k {
        .retries(out) => .retries(out),
        .name(out) => .name(out),
    }
}

command main | (exit: -i32) {
    let cfg = config();

    // Demand one item off the menu: the mirror of record projection.
    println(cfg.retries);            // 3
    println(cfg.name);               // "slant"
    println(loud(cfg).name);         // "slant!"

    // A cut delivers a request directly: `mu` names where the answer goes.
    println(mu ask(a: -i32) {
        cfg @ reroute(.retries(a))   // 3
    });

    0 @ exit
}
