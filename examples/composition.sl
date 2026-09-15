// The declaration square composes: a field can hold a type from any
// column, because a menu or form value is a value like any other.
//
//   data App     holds a menu        — codata stored in data
//   enum Slot    holds a form        — a consumer stored in a variant
//   menu Session answers an enum     — and one item is itself a menu
//   form CommandSink is fed an enum  — the consumer branches on data
//
// A consumer is a value like any other: a raw continuation sits in a
// positive record bare (CommandSink's `out`), named negatives likewise.

enum Cmd {
    Quit,
    Step(i64),
}

menu Config {
    retries: i64,
    name: String,
}

fn defaults() -> Config {
    mu Config {
        retries <= <3 | retries>,
        name <= <"slant" | name>,
    }
}

// ─── data holding a menu ───
data App {
    title: String,
    config: Config,
}

// ─── menu items answering an enum, and another menu ───
menu Session {
    next: Cmd,
    config: Config,
}

fn session(n: i64) -> Session {
    mu Session {
        next <= <(match (<(n, 0) | gt) { True => Step(n), False => Quit }) | next>,
        config <= <(mu Config { retries <= <n | retries>, name <= <"session" | name> }) | config>,
    }
}

// ─── a form fed an enum: the consumer branches on the data it receives ───
form CommandSink {
    cmd: Cmd,
    out: -String,
}

fn command_sink() -> CommandSink {
    select CommandSink {
        CommandSink { cmd, out } => match cmd {
            Quit => <"quit" | out>,
            Step(k) => <k | to_string | out>,
        },
    }
}

// ─── an enum payload holding a form value ───
enum Slot {
    Vacant,
    Holds(CommandSink),
}

command main | (exit: i32) / {IO} {
    let app = App { title: "demo", config: defaults() };
    match app {
        App { title, config } => {
            <title | println;
            <config.name | println;
        },
    };

    let s = <2 | session;
    <s.config.retries | println;
    match s.next {
        Quit => <"quit" | println,
        Step(k) => <k | println,
    };

    <mu String { ans <= <CommandSink { cmd: Step(7), out: ans } | command_sink()> } | println;

    match Holds(command_sink()) {
        Vacant => <"idle" | println,
        Holds(h) => <mu String { ans <= <CommandSink { cmd: Quit, out: ans } | h> } | println,
    };

    <0 | exit>
}
