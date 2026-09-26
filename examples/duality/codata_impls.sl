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
// stands in is what fixes it: in `<42 | deliver(s)`, `Self` is `+i64`.

cite list::List::*;
cite stream::Stream;
cite stream::count_from;

menu Config {
    retries: i64,
    name: String,
}

form Sink {
    value: i64,
    out: -i64,
}

spec Describe {
    func describe(self: Self) -> String;
}

impl Describe for Config {
    func describe(self: Config) -> String {
        <(self.name, " with ")
            | add
            | x => (x, <self.retries | fmt) | add
            | x => (x, " retries") | add
    }
}

impl Describe for Sink {
    func describe(self: Sink) -> String {
        "a sink for one number"
    }
}

// A bounded impl for a generic menu: describing a Stream<T> needs T
// displayable, and the dictionary composes at the use.
impl<+T: Display> Describe for Stream<T> {
    func describe(self: Stream<T>) -> String {
        <("stream starting ", <self.head | fmt) | add
    }
}

func config() -> Config {
    mu Config {
        retries <= <3 | retries>,
        name <= <"slant" | name>,
    }
}

func keeper() -> Sink {
    mu Sink {
        Sink { value, out } => <value | out>,
    }
}

// A bound discharged at codata types: `label` knows only that its argument
// is negative — a menu or a form, it cannot tell which.
func label<-T: Describe>(x: T) -> String {
    <x | describe
}

// A bounded negative function: `T` is fixed by the cut, and `fmt`'s
// dictionary travels in from the caller's side.
func emit<+T: Display>(out: String) <- T {
    fn(x: T) { <x | fmt | out> }
}

// A trait method that consumes `Self`. Dispatch reads the type the cut
// sends, so `<42 | deliver(s)` finds the `i64` impl.
spec Deliver {
    func deliver(out: String) <- Self;
}

impl Deliver for i64 {
    func deliver(out: String) <- i64 {
        fn(n: i64) { <("the number ", <n | fmt) | add | out> }
    }
}

impl Deliver for Bool {
    func deliver(out: String) <- Bool {
        fn(b: Bool) { <of b { True => "affirmative", False => "negative" } | out> }
    }
}

proc main | (exit: i32) / {IO} {
    <config() | describe | println;
    <keeper() | describe | println;
    <7 | count_from | describe | println;
    <config() | label | println;
    <keeper() | label | println;

    // the bounded negative function, at three different types
    <mu String { s <= <42 | (<s | emit)> } | println;
    <mu String { s <= <Cons(1, Cons(2, Nil)) | (<s | emit)> } | println;

    // the Self-consuming method, dispatched by what flows in
    <42 | deliver | println;
    <True | deliver | println;
    <0 | exit>
}
