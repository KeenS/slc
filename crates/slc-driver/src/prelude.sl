// The Slant prelude: what every program sees without asking.
//
// The driver appends this unit to the program before parsing — the user's
// source comes first, so its spans and line numbers are untouched — and
// everything below goes through the same checking and lowering as user code.
//
// Everything else the library offers lives in `stdlib/`, one module per
// file, reached by path — `list::length` — or brought in bare with `use`.

// ── IO: the effect the runtime handles ───────────────────────────────────
//
// Reaching outside the program is an effect like any other, and this is the
// one the runtime itself answers: `main` may leave `{IO}` undischarged, and
// the operation arrives at the handler the runtime installs around it.
// Nothing else about it is special — a program that installs its own handler
// sits nearer the operation and answers first, which is how output is
// mocked (`examples/io.sl`).
//
// `println` and `print` are the friendly front: they render any value and
// then perform `write_line`/`write` with the text.

effect IO {
    fn write(text: String) -> (,);
    fn write_line(text: String) -> (,);
}

// ── Display ──────────────────────────────────────────────────────────────
//
// User-facing formatting, as in Rust: `fmt` renders a value as the String a
// person should see — `fmt("hi")` is `hi`, unquoted — and `to_string` is
// the same act as a plain function.

trait Display {
    fn fmt(self: Self) -> String;
}

impl Display for i64 {
    fn fmt(self: i64) -> String { ⟨self | int_to_str }
}

impl Display for String {
    fn fmt(self: String) -> String { self }
}

impl Display for bool {
    fn fmt(self: bool) -> String { match self { true => { "true" }, _ => { "false" } } }
}

fn to_string<T: Display>(x: T) -> String { ⟨x | fmt }

// ── Logic ────────────────────────────────────────────────────────────────
//
// There is no `!`: negation is an ordinary function a `bool` flows into,
// `⟨b | not`. (The `_` arm stands for `false` until `bool` is declared.)

fn not(b: bool) -> bool {
    match b { true => false, _ => true }
}
