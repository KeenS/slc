// Traits meet the negative side, in both directions.
//
// First: codata carries impls. A trait attaches operations to a type,
// statically — and menus and forms are types like any other, so
// `impl Trait for Menu` and `impl Trait for Form` dispatch the way data
// does: concrete impls, a bounded impl for a generic menu, and a bound
// discharged at a codata type.
//
// Second (at the bottom): a *negative* function can be bounded, and a
// trait method can consume `Self` rather than receive it. Such a method
// takes no `self` parameter — a negative function's parameters are all
// continuations — so its `Self` is the type it consumes, and the cut it
// stands in is what fixes it: in `42 | deliver(s)`, `Self` is `+i64`.

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
        retries <= ⟨3 | retries⟩,
        name <= ⟨"slant" | name⟩,
    }
}

fn keeper() -> Sink {
    select Sink {
        Sink { value, out } => ⟨value | out⟩,
    }
}

// A bound discharged at codata types: `label` never knows its argument is
// a menu or a form.
fn label<T: Describe>(x: T) -> String {
    describe(x)
}

// A bounded negative function: `T` is fixed by the cut, and `fmt`'s
// dictionary travels in from the caller's side.
fn emit<T: Display>(out: -String) <- T {
    fn(x: T) { ⟨fmt(x) | out⟩ }
}

// A trait method that consumes `Self`. Dispatch reads the type the cut
// sends, so `42 | deliver(s)` finds the `i64` impl.
trait Deliver {
    fn deliver(out: -String) <- Self;
}

impl Deliver for i64 {
    fn deliver(out: -String) <- i64 {
        fn(n: +i64) { ⟨"the number " + fmt(n) | out⟩ }
    }
}

impl Deliver for bool {
    fn deliver(out: -String) <- bool {
        fn(b: +bool) { ⟨if b { "affirmative" } else { "negative" } | out⟩ }
    }
}

command main | (exit: -i32) {
    println(describe(config()));
    println(describe(keeper()));
    println(describe(count_from(7)));
    println(label(config()));
    println(label(keeper()));

    // the bounded negative function, at three different types
    println(mu String { s <= ⟨42 | emit(s)⟩ });
    println(mu String { s <= ⟨Cons(1, Cons(2, Nil)) | emit(s)⟩ });

    // the Self-consuming method, dispatched by what the cut sends
    println(mu String { s <= ⟨42 | deliver(s)⟩ });
    println(mu String { s <= ⟨true | deliver(s)⟩ });
    ⟨0 | exit⟩
}
