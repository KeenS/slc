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
// `println` and `print` are the friendly front: they render a value through
// `Display`, below, and then perform `write_line`/`write` with the text.

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

fn to_string<+T: Display>(x: T) -> String { ⟨x | fmt }

impl Display for i32 { fn fmt(self: i32) -> String { ⟨self | __display } }
impl Display for u32 { fn fmt(self: u32) -> String { ⟨self | __display } }
impl Display for u64 { fn fmt(self: u64) -> String { ⟨self | __display } }
impl Display for char { fn fmt(self: char) -> String { ⟨self | __display } }
impl Display for unit { fn fmt(self: unit) -> String { "(,)" } }
impl Display for File { fn fmt(self: File) -> String { ⟨self | __display } }

// Printing renders through `Display`, then performs `IO`'s operation.
fn println<+T: Display>(x: T) -> (,) / {IO} { ⟨(⟨x | fmt) | write_line }
fn print<+T: Display>(x: T) -> (,) / {IO} { ⟨(⟨x | fmt) | write }

// ── Display for anonymous data ───────────────────────────────────────────
//
// The unit, tuples and choices, up to eight components, each rendered as it
// is written: `(1, a)`, `::1(right)`, `(,)`.

impl Display for (,) {
    fn fmt(self: (,)) -> String { "(,)" }
}

impl<+A: Display, +B: Display> Display for (A, B) {
    fn fmt(self: (A, B)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ")" }
}

impl<+A: Display, +B: Display, +C: Display> Display for (A, B, C) {
    fn fmt(self: (A, B, C)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ", " + (⟨self.2 | fmt) + ")" }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display> Display for (A, B, C, D) {
    fn fmt(self: (A, B, C, D)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ", " + (⟨self.2 | fmt) + ", " + (⟨self.3 | fmt) + ")" }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display> Display for (A, B, C, D, E) {
    fn fmt(self: (A, B, C, D, E)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ", " + (⟨self.2 | fmt) + ", " + (⟨self.3 | fmt) + ", " + (⟨self.4 | fmt) + ")" }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display, +F: Display> Display for (A, B, C, D, E, F) {
    fn fmt(self: (A, B, C, D, E, F)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ", " + (⟨self.2 | fmt) + ", " + (⟨self.3 | fmt) + ", " + (⟨self.4 | fmt) + ", " + (⟨self.5 | fmt) + ")" }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display, +F: Display, +G: Display> Display for (A, B, C, D, E, F, G) {
    fn fmt(self: (A, B, C, D, E, F, G)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ", " + (⟨self.2 | fmt) + ", " + (⟨self.3 | fmt) + ", " + (⟨self.4 | fmt) + ", " + (⟨self.5 | fmt) + ", " + (⟨self.6 | fmt) + ")" }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display, +F: Display, +G: Display, +H: Display> Display for (A, B, C, D, E, F, G, H) {
    fn fmt(self: (A, B, C, D, E, F, G, H)) -> String { "(" + (⟨self.0 | fmt) + ", " + (⟨self.1 | fmt) + ", " + (⟨self.2 | fmt) + ", " + (⟨self.3 | fmt) + ", " + (⟨self.4 | fmt) + ", " + (⟨self.5 | fmt) + ", " + (⟨self.6 | fmt) + ", " + (⟨self.7 | fmt) + ")" }
}

impl<+A: Display, +B: Display> Display for (A | B) {
    fn fmt(self: (A | B)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")" } }
}

impl<+A: Display, +B: Display, +C: Display> Display for (A | B | C) {
    fn fmt(self: (A | B | C)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")", ::2(x) => "::2(" + (⟨x | fmt) + ")" } }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display> Display for (A | B | C | D) {
    fn fmt(self: (A | B | C | D)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")", ::2(x) => "::2(" + (⟨x | fmt) + ")", ::3(x) => "::3(" + (⟨x | fmt) + ")" } }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display> Display for (A | B | C | D | E) {
    fn fmt(self: (A | B | C | D | E)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")", ::2(x) => "::2(" + (⟨x | fmt) + ")", ::3(x) => "::3(" + (⟨x | fmt) + ")", ::4(x) => "::4(" + (⟨x | fmt) + ")" } }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display, +F: Display> Display for (A | B | C | D | E | F) {
    fn fmt(self: (A | B | C | D | E | F)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")", ::2(x) => "::2(" + (⟨x | fmt) + ")", ::3(x) => "::3(" + (⟨x | fmt) + ")", ::4(x) => "::4(" + (⟨x | fmt) + ")", ::5(x) => "::5(" + (⟨x | fmt) + ")" } }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display, +F: Display, +G: Display> Display for (A | B | C | D | E | F | G) {
    fn fmt(self: (A | B | C | D | E | F | G)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")", ::2(x) => "::2(" + (⟨x | fmt) + ")", ::3(x) => "::3(" + (⟨x | fmt) + ")", ::4(x) => "::4(" + (⟨x | fmt) + ")", ::5(x) => "::5(" + (⟨x | fmt) + ")", ::6(x) => "::6(" + (⟨x | fmt) + ")" } }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display, +F: Display, +G: Display, +H: Display> Display for (A | B | C | D | E | F | G | H) {
    fn fmt(self: (A | B | C | D | E | F | G | H)) -> String { match self { ::0(x) => "::0(" + (⟨x | fmt) + ")", ::1(x) => "::1(" + (⟨x | fmt) + ")", ::2(x) => "::2(" + (⟨x | fmt) + ")", ::3(x) => "::3(" + (⟨x | fmt) + ")", ::4(x) => "::4(" + (⟨x | fmt) + ")", ::5(x) => "::5(" + (⟨x | fmt) + ")", ::6(x) => "::6(" + (⟨x | fmt) + ")", ::7(x) => "::7(" + (⟨x | fmt) + ")" } }
}

// ── Logic ────────────────────────────────────────────────────────────────
//
// There is no `!`: negation is an ordinary function a `bool` flows into,
// `⟨b | not`. (The `_` arm stands for `false` until `bool` is declared.)

fn not(b: bool) -> bool {
    match b { true => false, _ => true }
}
