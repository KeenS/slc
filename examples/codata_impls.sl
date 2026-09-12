// Codata carries impls: `impl Trait for Menu` and `impl Trait for Form`.
//
// A trait attaches operations to a type, statically — and menus and forms
// are types like any other, so the negative side dispatches the same way
// data does: concrete impls, a bounded impl for a generic menu, and a
// bound discharged at a codata type.

menu Config {
    retries: i64,
    name: String,
}

form Sink {
    value: i64,
    out: -i64,
}

trait Describe {
    fn describe(self: +Self) -> String;
}

impl Describe for Config {
    fn describe(self: +Config) -> String {
        self.name + " with " + fmt(self.retries) + " retries"
    }
}

impl Describe for Sink {
    fn describe(self: +Sink) -> String {
        "a sink for one number"
    }
}

// A bounded impl for a generic menu: describing a Stream<T> needs T
// displayable, and the dictionary composes at the use.
impl<T: Display> Describe for Stream<T> {
    fn describe(self: +Stream<T>) -> String {
        "stream starting " + fmt(self.head)
    }
}

fn config() -> Config {
    mu Config {
        retries <= 3 @ retries,
        name <= "slant" @ name,
    }
}

fn keeper() -> Sink {
    select Sink {
        Sink { value, out } => value @ out,
    }
}

// A bound discharged at codata types: `label` never knows its argument is
// a menu or a form.
fn label<T: Describe>(x: T) -> String {
    describe(x)
}

command main | (exit: -i32) {
    println(describe(config()));
    println(describe(keeper()));
    println(describe(count_from(7)));
    println(label(config()));
    println(label(keeper()));
    0 @ exit
}
