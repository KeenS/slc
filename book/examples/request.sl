// of takes a request apart. Each arm answers with a request.

menu Config {
    retries: i64,
    name: String,
}

func config() -> Config {
    mu Config {
        retries <= <3 | retries>,
        name <= <"SLC" | name>,
    }
}

func reroute(k: -Config) -> -Config {
    of k {
        .retries(out) <= .retries(out),
        .name(out) <= .name(out),
    }
}

proc main | (exit: i32) / {IO} {
    <mu i64 { a <= <config() | (<.retries(a) | reroute)> } | println;
    <0 | exit>
}
