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

// ── Arithmetic and comparison ────────────────────────────────────────────
//
// Each operator is a trait method over a builtin beneath it: `⟨(a, b) | add`
// dispatches on the type the two agree on, and an integer literal takes its
// width from the other operand.

trait Add { fn add(self: Self, other: Self) -> Self; }
impl Add for i64 { fn add(self: i64, other: i64) -> i64 { ⟨(self, other) | __add } }
impl Add for i32 { fn add(self: i32, other: i32) -> i32 { ⟨(self, other) | __add } }
impl Add for u64 { fn add(self: u64, other: u64) -> u64 { ⟨(self, other) | __add } }
impl Add for u32 { fn add(self: u32, other: u32) -> u32 { ⟨(self, other) | __add } }
impl Add for String { fn add(self: String, other: String) -> String { ⟨(self, other) | __add } }

trait Sub { fn sub(self: Self, other: Self) -> Self; }
impl Sub for i64 { fn sub(self: i64, other: i64) -> i64 { ⟨(self, other) | __sub } }
impl Sub for i32 { fn sub(self: i32, other: i32) -> i32 { ⟨(self, other) | __sub } }
impl Sub for u64 { fn sub(self: u64, other: u64) -> u64 { ⟨(self, other) | __sub } }
impl Sub for u32 { fn sub(self: u32, other: u32) -> u32 { ⟨(self, other) | __sub } }

trait Mul { fn mul(self: Self, other: Self) -> Self; }
impl Mul for i64 { fn mul(self: i64, other: i64) -> i64 { ⟨(self, other) | __mul } }
impl Mul for i32 { fn mul(self: i32, other: i32) -> i32 { ⟨(self, other) | __mul } }
impl Mul for u64 { fn mul(self: u64, other: u64) -> u64 { ⟨(self, other) | __mul } }
impl Mul for u32 { fn mul(self: u32, other: u32) -> u32 { ⟨(self, other) | __mul } }

trait Div { fn div(self: Self, other: Self) -> Self; }
impl Div for i64 { fn div(self: i64, other: i64) -> i64 { ⟨(self, other) | __div } }
impl Div for i32 { fn div(self: i32, other: i32) -> i32 { ⟨(self, other) | __div } }
impl Div for u64 { fn div(self: u64, other: u64) -> u64 { ⟨(self, other) | __div } }
impl Div for u32 { fn div(self: u32, other: u32) -> u32 { ⟨(self, other) | __div } }

trait Rem { fn rem(self: Self, other: Self) -> Self; }
impl Rem for i64 { fn rem(self: i64, other: i64) -> i64 { ⟨(self, other) | __rem } }
impl Rem for i32 { fn rem(self: i32, other: i32) -> i32 { ⟨(self, other) | __rem } }
impl Rem for u64 { fn rem(self: u64, other: u64) -> u64 { ⟨(self, other) | __rem } }
impl Rem for u32 { fn rem(self: u32, other: u32) -> u32 { ⟨(self, other) | __rem } }

trait Neg { fn neg(self: Self) -> Self; }
impl Neg for i64 { fn neg(self: i64) -> i64 { ⟨self | __neg } }
impl Neg for i32 { fn neg(self: i32) -> i32 { ⟨self | __neg } }

trait Eq {
    fn eq(self: Self, other: Self) -> bool;
    fn ne(self: Self, other: Self) -> bool;
}
impl Eq for i64 {
    fn eq(self: i64, other: i64) -> bool { ⟨(self, other) | __eq }
    fn ne(self: i64, other: i64) -> bool { ⟨(self, other) | __ne }
}
impl Eq for i32 {
    fn eq(self: i32, other: i32) -> bool { ⟨(self, other) | __eq }
    fn ne(self: i32, other: i32) -> bool { ⟨(self, other) | __ne }
}
impl Eq for u64 {
    fn eq(self: u64, other: u64) -> bool { ⟨(self, other) | __eq }
    fn ne(self: u64, other: u64) -> bool { ⟨(self, other) | __ne }
}
impl Eq for u32 {
    fn eq(self: u32, other: u32) -> bool { ⟨(self, other) | __eq }
    fn ne(self: u32, other: u32) -> bool { ⟨(self, other) | __ne }
}
impl Eq for char {
    fn eq(self: char, other: char) -> bool { ⟨(self, other) | __eq }
    fn ne(self: char, other: char) -> bool { ⟨(self, other) | __ne }
}
impl Eq for String {
    fn eq(self: String, other: String) -> bool { ⟨(self, other) | __eq }
    fn ne(self: String, other: String) -> bool { ⟨(self, other) | __ne }
}
impl Eq for bool {
    fn eq(self: bool, other: bool) -> bool { ⟨(self, other) | __eq }
    fn ne(self: bool, other: bool) -> bool { ⟨(self, other) | __ne }
}

trait Ord {
    fn lt(self: Self, other: Self) -> bool;
    fn gt(self: Self, other: Self) -> bool;
    fn le(self: Self, other: Self) -> bool;
    fn ge(self: Self, other: Self) -> bool;
}
impl Ord for i64 {
    fn lt(self: i64, other: i64) -> bool { ⟨(self, other) | __lt }
    fn gt(self: i64, other: i64) -> bool { ⟨(self, other) | __gt }
    fn le(self: i64, other: i64) -> bool { ⟨(self, other) | __le }
    fn ge(self: i64, other: i64) -> bool { ⟨(self, other) | __ge }
}
impl Ord for i32 {
    fn lt(self: i32, other: i32) -> bool { ⟨(self, other) | __lt }
    fn gt(self: i32, other: i32) -> bool { ⟨(self, other) | __gt }
    fn le(self: i32, other: i32) -> bool { ⟨(self, other) | __le }
    fn ge(self: i32, other: i32) -> bool { ⟨(self, other) | __ge }
}
impl Ord for u64 {
    fn lt(self: u64, other: u64) -> bool { ⟨(self, other) | __lt }
    fn gt(self: u64, other: u64) -> bool { ⟨(self, other) | __gt }
    fn le(self: u64, other: u64) -> bool { ⟨(self, other) | __le }
    fn ge(self: u64, other: u64) -> bool { ⟨(self, other) | __ge }
}
impl Ord for u32 {
    fn lt(self: u32, other: u32) -> bool { ⟨(self, other) | __lt }
    fn gt(self: u32, other: u32) -> bool { ⟨(self, other) | __gt }
    fn le(self: u32, other: u32) -> bool { ⟨(self, other) | __le }
    fn ge(self: u32, other: u32) -> bool { ⟨(self, other) | __ge }
}
impl Ord for char {
    fn lt(self: char, other: char) -> bool { ⟨(self, other) | __lt }
    fn gt(self: char, other: char) -> bool { ⟨(self, other) | __gt }
    fn le(self: char, other: char) -> bool { ⟨(self, other) | __le }
    fn ge(self: char, other: char) -> bool { ⟨(self, other) | __ge }
}
impl Ord for String {
    fn lt(self: String, other: String) -> bool { ⟨(self, other) | __lt }
    fn gt(self: String, other: String) -> bool { ⟨(self, other) | __gt }
    fn le(self: String, other: String) -> bool { ⟨(self, other) | __le }
    fn ge(self: String, other: String) -> bool { ⟨(self, other) | __ge }
}
impl Ord for bool {
    fn lt(self: bool, other: bool) -> bool { ⟨(self, other) | __lt }
    fn gt(self: bool, other: bool) -> bool { ⟨(self, other) | __gt }
    fn le(self: bool, other: bool) -> bool { ⟨(self, other) | __le }
    fn ge(self: bool, other: bool) -> bool { ⟨(self, other) | __ge }
}

// The character of a `String` at a position, failing at run time when the
// position is out of range; `char_at` offers that outcome to a continuation.
fn index(s: String, i: i64) -> char { ⟨(s, i) | __index }

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
