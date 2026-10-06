// data carries every field. form wants every field.
// enum is one variant. menu answers one item.

data Pair {
    left: i64,
    right: i64,
}

form Total {
    left: i64,
    right: i64,
}

enum Colour {
    Red,
    Green,
}

menu Config {
    retries: i64,
    name: String,
}

func sum(p: Pair) -> i64 {
    of p {
        Pair { left, right } => <(left, right) | add,
    }
}

func total(out: -i64) -> Total {
    mu Total {
        Total { left, right } => <(left, right) | add | out>,
    }
}

func name(c: Colour) -> String {
    of c {
        Red => "red",
        Green => "green",
    }
}

func config() -> Config {
    mu Config {
        retries <= <3 | retries>,
        name <= <"SLC" | name>,
    }
}

proc main | (exit: i32) / {IO} {
    let p = Pair { left: 2, right: 40 };
    <p.left | println;
    <p | sum | println;
    <mu i64 { answer <= <Total { left: 2, right: 40 } | (<answer | total)> } | println;
    <Colour::Green | name | println;
    <config().retries | println;
    <config().name | println;
    <0 | exit>
}
