// The SLC prelude: what every program sees without asking.
//
// The driver appends this unit to the program before parsing — the user's
// source comes first, so its spans and line numbers are untouched — and
// everything below goes through the same checking and lowering as user code.
//
// Everything else the library offers lives in `stdlib/`, one module per
// file, reached by path — `list::length` — or brought in bare with `cite`.

// ── IO: the effect the runtime handles ───────────────────────────────────
//
// Reaching outside the program is an effect like any other, and this is the
// one the runtime itself answers: `main` may leave `{IO}` undischarged, and
// the operation arrives at the handler the runtime installs around it.
// Nothing else about it is special — a program that installs its own handler
// sits nearer the operation and answers first, which is how output is
// mocked (`examples/effects/io.sl`).
//
// `println` and `print` are the friendly front: they render a value through
// `Display`, below, and then perform `write_line`/`write` with the text.

hook IO {
    func write(text: String) -> (,);
    func write_line(text: String) -> (,);
}

// ── Bool ─────────────────────────────────────────────────────────────────
//
// An ordinary enum, so a `of` on one is exhaustive the way a match on any
// enum is. The builtins that answer yes or no answer with it.

enum Bool { False, True }

// ── Display ──────────────────────────────────────────────────────────────
//
// User-facing formatting, as in Rust: `fmt` renders a value as the String a
// person should see — `fmt("hi")` is `hi`, unquoted — and `to_string` is
// the same act as a plain function.

spec Display {
    func fmt(self: Self) -> String;
}

impl Display for i64 {
    func fmt(self: i64) -> String { <self | int_to_str }
}

impl Display for i8 { func fmt(self: i8) -> String { <self | __display } }
impl Display for u8 { func fmt(self: u8) -> String { <self | __display } }
impl Display for f32 { func fmt(self: f32) -> String { <self | __display } }
impl Display for f64 { func fmt(self: f64) -> String { <self | __display } }

impl Display for String {
    func fmt(self: String) -> String { self }
}

impl Display for Bool {
    func fmt(self: Bool) -> String { of self { True => { "true" }, False => { "false" } } }
}

func to_string<+T: Display>(x: T) -> String { <x | fmt }

impl Display for i32 { func fmt(self: i32) -> String { <self | __display } }
impl Display for u32 { func fmt(self: u32) -> String { <self | __display } }
impl Display for u64 { func fmt(self: u64) -> String { <self | __display } }
impl Display for char { func fmt(self: char) -> String { <self | __display } }
impl Display for File { func fmt(self: File) -> String { <self | __display } }

// Printing renders through `Display`, then performs `IO`'s operation.
func println<+T: Display>(x: T) -> (,) / {IO} { <(<x | fmt) | write_line }
func print<+T: Display>(x: T) -> (,) / {IO} { <(<x | fmt) | write }

// ── Arithmetic and comparison ────────────────────────────────────────────
//
// Each operator is a trait method over a builtin beneath it: `<(a, b) | add`
// dispatches on the type the two agree on, and an integer literal takes its
// width from the other operand.

spec Add { func add(self: Self, other: Self) -> Self; }
impl Add for i64 { func add(self: i64, other: i64) -> i64 { <(self, other) | __add } }
impl Add for i8 { func add(self: i8, other: i8) -> i8 { <(self, other) | __add } }
impl Add for i32 { func add(self: i32, other: i32) -> i32 { <(self, other) | __add } }
impl Add for u8 { func add(self: u8, other: u8) -> u8 { <(self, other) | __add } }
impl Add for u64 { func add(self: u64, other: u64) -> u64 { <(self, other) | __add } }
impl Add for u32 { func add(self: u32, other: u32) -> u32 { <(self, other) | __add } }
impl Add for f32 { func add(self: f32, other: f32) -> f32 { <(self, other) | __add } }
impl Add for f64 { func add(self: f64, other: f64) -> f64 { <(self, other) | __add } }
impl Add for String { func add(self: String, other: String) -> String { <(self, other) | __add } }

spec Sub { func sub(self: Self, other: Self) -> Self; }
impl Sub for i64 { func sub(self: i64, other: i64) -> i64 { <(self, other) | __sub } }
impl Sub for i8 { func sub(self: i8, other: i8) -> i8 { <(self, other) | __sub } }
impl Sub for i32 { func sub(self: i32, other: i32) -> i32 { <(self, other) | __sub } }
impl Sub for u8 { func sub(self: u8, other: u8) -> u8 { <(self, other) | __sub } }
impl Sub for u64 { func sub(self: u64, other: u64) -> u64 { <(self, other) | __sub } }
impl Sub for u32 { func sub(self: u32, other: u32) -> u32 { <(self, other) | __sub } }
impl Sub for f32 { func sub(self: f32, other: f32) -> f32 { <(self, other) | __sub } }
impl Sub for f64 { func sub(self: f64, other: f64) -> f64 { <(self, other) | __sub } }

spec Mul { func mul(self: Self, other: Self) -> Self; }
impl Mul for i64 { func mul(self: i64, other: i64) -> i64 { <(self, other) | __mul } }
impl Mul for i8 { func mul(self: i8, other: i8) -> i8 { <(self, other) | __mul } }
impl Mul for i32 { func mul(self: i32, other: i32) -> i32 { <(self, other) | __mul } }
impl Mul for u8 { func mul(self: u8, other: u8) -> u8 { <(self, other) | __mul } }
impl Mul for u64 { func mul(self: u64, other: u64) -> u64 { <(self, other) | __mul } }
impl Mul for u32 { func mul(self: u32, other: u32) -> u32 { <(self, other) | __mul } }
impl Mul for f32 { func mul(self: f32, other: f32) -> f32 { <(self, other) | __mul } }
impl Mul for f64 { func mul(self: f64, other: f64) -> f64 { <(self, other) | __mul } }

spec Div { func div(self: Self, other: Self) -> Self; }
impl Div for i64 { func div(self: i64, other: i64) -> i64 { <(self, other) | __div } }
impl Div for i8 { func div(self: i8, other: i8) -> i8 { <(self, other) | __div } }
impl Div for i32 { func div(self: i32, other: i32) -> i32 { <(self, other) | __div } }
impl Div for u8 { func div(self: u8, other: u8) -> u8 { <(self, other) | __div } }
impl Div for u64 { func div(self: u64, other: u64) -> u64 { <(self, other) | __div } }
impl Div for u32 { func div(self: u32, other: u32) -> u32 { <(self, other) | __div } }
impl Div for f32 { func div(self: f32, other: f32) -> f32 { <(self, other) | __div } }
impl Div for f64 { func div(self: f64, other: f64) -> f64 { <(self, other) | __div } }

spec Rem { func rem(self: Self, other: Self) -> Self; }
impl Rem for i64 { func rem(self: i64, other: i64) -> i64 { <(self, other) | __rem } }
impl Rem for i8 { func rem(self: i8, other: i8) -> i8 { <(self, other) | __rem } }
impl Rem for i32 { func rem(self: i32, other: i32) -> i32 { <(self, other) | __rem } }
impl Rem for u8 { func rem(self: u8, other: u8) -> u8 { <(self, other) | __rem } }
impl Rem for u64 { func rem(self: u64, other: u64) -> u64 { <(self, other) | __rem } }
impl Rem for u32 { func rem(self: u32, other: u32) -> u32 { <(self, other) | __rem } }
impl Rem for f32 { func rem(self: f32, other: f32) -> f32 { <(self, other) | __rem } }
impl Rem for f64 { func rem(self: f64, other: f64) -> f64 { <(self, other) | __rem } }

spec Neg { func neg(self: Self) -> Self; }
impl Neg for i64 { func neg(self: i64) -> i64 { <self | __neg } }
impl Neg for i8 { func neg(self: i8) -> i8 { <self | __neg } }
impl Neg for i32 { func neg(self: i32) -> i32 { <self | __neg } }
impl Neg for f32 { func neg(self: f32) -> f32 { <self | __neg } }
impl Neg for f64 { func neg(self: f64) -> f64 { <self | __neg } }

// Square root's domain is the non-negative reals, including `-0.0`. A
// negative number overflows. Absolute value, floor, and ceiling are total.
spec Sqrt { func sqrt(self: Self) -> Self; }
impl Sqrt for f64 { func sqrt(self: f64) -> f64 { <self | __sqrt } }
impl Sqrt for f32 { func sqrt(self: f32) -> f32 { <self | __sqrt } }

spec Abs { func abs(self: Self) -> Self; }
impl Abs for f64 { func abs(self: f64) -> f64 { <self | __abs } }
impl Abs for f32 { func abs(self: f32) -> f32 { <self | __abs } }

spec Floor { func floor(self: Self) -> Self; }
impl Floor for f64 { func floor(self: f64) -> f64 { <self | __floor } }
impl Floor for f32 { func floor(self: f32) -> f32 { <self | __floor } }

spec Ceil { func ceil(self: Self) -> Self; }
impl Ceil for f64 { func ceil(self: f64) -> f64 { <self | __ceil } }
impl Ceil for f32 { func ceil(self: f32) -> f32 { <self | __ceil } }

// A value moves between integer widths, and between those widths and
// `f32` and `f64`, by `Into`. The expected type picks the destination.
// The number is kept when it fits there exactly; otherwise the conversion
// overflows, as `add` does. There is no truncating or rounding cast.
spec Into<+U> {
    func into(self: Self) -> U;
}
impl Into<i32> for i8 { func into(self: i8) -> i32 { <self | __to_i32 } }
impl Into<i64> for i8 { func into(self: i8) -> i64 { <self | __to_i64 } }
impl Into<u8> for i8 { func into(self: i8) -> u8 { <self | __to_u8 } }
impl Into<u32> for i8 { func into(self: i8) -> u32 { <self | __to_u32 } }
impl Into<u64> for i8 { func into(self: i8) -> u64 { <self | __to_u64 } }
impl Into<i8> for i32 { func into(self: i32) -> i8 { <self | __to_i8 } }
impl Into<i64> for i32 { func into(self: i32) -> i64 { <self | __to_i64 } }
impl Into<u8> for i32 { func into(self: i32) -> u8 { <self | __to_u8 } }
impl Into<u32> for i32 { func into(self: i32) -> u32 { <self | __to_u32 } }
impl Into<u64> for i32 { func into(self: i32) -> u64 { <self | __to_u64 } }
impl Into<i8> for i64 { func into(self: i64) -> i8 { <self | __to_i8 } }
impl Into<i32> for i64 { func into(self: i64) -> i32 { <self | __to_i32 } }
impl Into<u8> for i64 { func into(self: i64) -> u8 { <self | __to_u8 } }
impl Into<u32> for i64 { func into(self: i64) -> u32 { <self | __to_u32 } }
impl Into<u64> for i64 { func into(self: i64) -> u64 { <self | __to_u64 } }
impl Into<i8> for u8 { func into(self: u8) -> i8 { <self | __to_i8 } }
impl Into<i32> for u8 { func into(self: u8) -> i32 { <self | __to_i32 } }
impl Into<i64> for u8 { func into(self: u8) -> i64 { <self | __to_i64 } }
impl Into<u32> for u8 { func into(self: u8) -> u32 { <self | __to_u32 } }
impl Into<u64> for u8 { func into(self: u8) -> u64 { <self | __to_u64 } }
impl Into<i8> for u32 { func into(self: u32) -> i8 { <self | __to_i8 } }
impl Into<i32> for u32 { func into(self: u32) -> i32 { <self | __to_i32 } }
impl Into<i64> for u32 { func into(self: u32) -> i64 { <self | __to_i64 } }
impl Into<u8> for u32 { func into(self: u32) -> u8 { <self | __to_u8 } }
impl Into<u64> for u32 { func into(self: u32) -> u64 { <self | __to_u64 } }
impl Into<i8> for u64 { func into(self: u64) -> i8 { <self | __to_i8 } }
impl Into<i32> for u64 { func into(self: u64) -> i32 { <self | __to_i32 } }
impl Into<i64> for u64 { func into(self: u64) -> i64 { <self | __to_i64 } }
impl Into<u8> for u64 { func into(self: u64) -> u8 { <self | __to_u8 } }
impl Into<u32> for u64 { func into(self: u64) -> u32 { <self | __to_u32 } }
impl Into<f64> for i8 { func into(self: i8) -> f64 { <self | __to_f64 } }
impl Into<f32> for i8 { func into(self: i8) -> f32 { <self | __to_f32 } }
impl Into<f64> for i32 { func into(self: i32) -> f64 { <self | __to_f64 } }
impl Into<f32> for i32 { func into(self: i32) -> f32 { <self | __to_f32 } }
impl Into<f64> for i64 { func into(self: i64) -> f64 { <self | __to_f64 } }
impl Into<f32> for i64 { func into(self: i64) -> f32 { <self | __to_f32 } }
impl Into<f64> for u8 { func into(self: u8) -> f64 { <self | __to_f64 } }
impl Into<f32> for u8 { func into(self: u8) -> f32 { <self | __to_f32 } }
impl Into<f64> for u32 { func into(self: u32) -> f64 { <self | __to_f64 } }
impl Into<f32> for u32 { func into(self: u32) -> f32 { <self | __to_f32 } }
impl Into<f64> for u64 { func into(self: u64) -> f64 { <self | __to_f64 } }
impl Into<f32> for u64 { func into(self: u64) -> f32 { <self | __to_f32 } }
impl Into<i8> for f64 { func into(self: f64) -> i8 { <self | __to_i8 } }
impl Into<i32> for f64 { func into(self: f64) -> i32 { <self | __to_i32 } }
impl Into<i64> for f64 { func into(self: f64) -> i64 { <self | __to_i64 } }
impl Into<u8> for f64 { func into(self: f64) -> u8 { <self | __to_u8 } }
impl Into<u32> for f64 { func into(self: f64) -> u32 { <self | __to_u32 } }
impl Into<u64> for f64 { func into(self: f64) -> u64 { <self | __to_u64 } }
impl Into<f32> for f64 { func into(self: f64) -> f32 { <self | __to_f32 } }
impl Into<i8> for f32 { func into(self: f32) -> i8 { <self | __to_i8 } }
impl Into<i32> for f32 { func into(self: f32) -> i32 { <self | __to_i32 } }
impl Into<i64> for f32 { func into(self: f32) -> i64 { <self | __to_i64 } }
impl Into<u8> for f32 { func into(self: f32) -> u8 { <self | __to_u8 } }
impl Into<u32> for f32 { func into(self: f32) -> u32 { <self | __to_u32 } }
impl Into<u64> for f32 { func into(self: f32) -> u64 { <self | __to_u64 } }
impl Into<f64> for f32 { func into(self: f32) -> f64 { <self | __to_f64 } }

spec Eq {
    func eq(self: Self, other: Self) -> Bool;
    func ne(self: Self, other: Self) -> Bool { <(<(self, other) | eq) | not }
}
impl Eq for i64 {
    func eq(self: i64, other: i64) -> Bool { <(self, other) | __eq }
    func ne(self: i64, other: i64) -> Bool { <(self, other) | __ne }
}
impl Eq for i8 {
    func eq(self: i8, other: i8) -> Bool { <(self, other) | __eq }
    func ne(self: i8, other: i8) -> Bool { <(self, other) | __ne }
}
impl Eq for i32 {
    func eq(self: i32, other: i32) -> Bool { <(self, other) | __eq }
    func ne(self: i32, other: i32) -> Bool { <(self, other) | __ne }
}
impl Eq for u8 {
    func eq(self: u8, other: u8) -> Bool { <(self, other) | __eq }
    func ne(self: u8, other: u8) -> Bool { <(self, other) | __ne }
}
impl Eq for u64 {
    func eq(self: u64, other: u64) -> Bool { <(self, other) | __eq }
    func ne(self: u64, other: u64) -> Bool { <(self, other) | __ne }
}
impl Eq for u32 {
    func eq(self: u32, other: u32) -> Bool { <(self, other) | __eq }
    func ne(self: u32, other: u32) -> Bool { <(self, other) | __ne }
}
impl Eq for f32 {
    func eq(self: f32, other: f32) -> Bool { <(self, other) | __eq }
    func ne(self: f32, other: f32) -> Bool { <(self, other) | __ne }
}
impl Eq for f64 {
    func eq(self: f64, other: f64) -> Bool { <(self, other) | __eq }
    func ne(self: f64, other: f64) -> Bool { <(self, other) | __ne }
}
impl Eq for char {
    func eq(self: char, other: char) -> Bool { <(self, other) | __eq }
    func ne(self: char, other: char) -> Bool { <(self, other) | __ne }
}
impl Eq for String {
    func eq(self: String, other: String) -> Bool { <(self, other) | __eq }
    func ne(self: String, other: String) -> Bool { <(self, other) | __ne }
}
impl Eq for Bool {
    func eq(self: Bool, other: Bool) -> Bool { <(self, other) | __eq }
    func ne(self: Bool, other: Bool) -> Bool { <(self, other) | __ne }
}

spec Ord {
    func lt(self: Self, other: Self) -> Bool;
    func gt(self: Self, other: Self) -> Bool;
    func le(self: Self, other: Self) -> Bool;
    func ge(self: Self, other: Self) -> Bool;
}
impl Ord for i64 {
    func lt(self: i64, other: i64) -> Bool { <(self, other) | __lt }
    func gt(self: i64, other: i64) -> Bool { <(self, other) | __gt }
    func le(self: i64, other: i64) -> Bool { <(self, other) | __le }
    func ge(self: i64, other: i64) -> Bool { <(self, other) | __ge }
}
impl Ord for i8 {
    func lt(self: i8, other: i8) -> Bool { <(self, other) | __lt }
    func gt(self: i8, other: i8) -> Bool { <(self, other) | __gt }
    func le(self: i8, other: i8) -> Bool { <(self, other) | __le }
    func ge(self: i8, other: i8) -> Bool { <(self, other) | __ge }
}
impl Ord for i32 {
    func lt(self: i32, other: i32) -> Bool { <(self, other) | __lt }
    func gt(self: i32, other: i32) -> Bool { <(self, other) | __gt }
    func le(self: i32, other: i32) -> Bool { <(self, other) | __le }
    func ge(self: i32, other: i32) -> Bool { <(self, other) | __ge }
}
impl Ord for u8 {
    func lt(self: u8, other: u8) -> Bool { <(self, other) | __lt }
    func gt(self: u8, other: u8) -> Bool { <(self, other) | __gt }
    func le(self: u8, other: u8) -> Bool { <(self, other) | __le }
    func ge(self: u8, other: u8) -> Bool { <(self, other) | __ge }
}
impl Ord for u64 {
    func lt(self: u64, other: u64) -> Bool { <(self, other) | __lt }
    func gt(self: u64, other: u64) -> Bool { <(self, other) | __gt }
    func le(self: u64, other: u64) -> Bool { <(self, other) | __le }
    func ge(self: u64, other: u64) -> Bool { <(self, other) | __ge }
}
impl Ord for u32 {
    func lt(self: u32, other: u32) -> Bool { <(self, other) | __lt }
    func gt(self: u32, other: u32) -> Bool { <(self, other) | __gt }
    func le(self: u32, other: u32) -> Bool { <(self, other) | __le }
    func ge(self: u32, other: u32) -> Bool { <(self, other) | __ge }
}
impl Ord for f32 {
    func lt(self: f32, other: f32) -> Bool { <(self, other) | __lt }
    func gt(self: f32, other: f32) -> Bool { <(self, other) | __gt }
    func le(self: f32, other: f32) -> Bool { <(self, other) | __le }
    func ge(self: f32, other: f32) -> Bool { <(self, other) | __ge }
}
impl Ord for f64 {
    func lt(self: f64, other: f64) -> Bool { <(self, other) | __lt }
    func gt(self: f64, other: f64) -> Bool { <(self, other) | __gt }
    func le(self: f64, other: f64) -> Bool { <(self, other) | __le }
    func ge(self: f64, other: f64) -> Bool { <(self, other) | __ge }
}
impl Ord for char {
    func lt(self: char, other: char) -> Bool { <(self, other) | __lt }
    func gt(self: char, other: char) -> Bool { <(self, other) | __gt }
    func le(self: char, other: char) -> Bool { <(self, other) | __le }
    func ge(self: char, other: char) -> Bool { <(self, other) | __ge }
}
impl Ord for String {
    func lt(self: String, other: String) -> Bool { <(self, other) | __lt }
    func gt(self: String, other: String) -> Bool { <(self, other) | __gt }
    func le(self: String, other: String) -> Bool { <(self, other) | __le }
    func ge(self: String, other: String) -> Bool { <(self, other) | __ge }
}
impl Ord for Bool {
    func lt(self: Bool, other: Bool) -> Bool { <(self, other) | __lt }
    func gt(self: Bool, other: Bool) -> Bool { <(self, other) | __gt }
    func le(self: Bool, other: Bool) -> Bool { <(self, other) | __le }
    func ge(self: Bool, other: Bool) -> Bool { <(self, other) | __ge }
}

// The machine word's product in the ring of its 64-bit patterns, and the
// exclusive or of two such words. Checked `mul` refuses what this wraps.
func wrapping_mul(a: i64, b: i64) -> i64 { <(a, b) | __wrapping_mul }
func xor(a: i64, b: i64) -> i64 { <(a, b) | __xor }

// A non-negative `u64` derived from the value. Equal values hash equal.
// The mix multiplies the word by the bit pattern of `0x9E3779B97F4A7C15`
// and clears the high bit, so `rem` of a hash is a slot in `0 .. width`.
// A `String` is FNV-1a over its scalar values, from `char_to_code`.
func sign_bit() -> i64 { <(-9_223_372_036_854_775_807, 1) | sub }

func clear_sign(n: i64) -> i64 {
    of (<(n, 0) | lt) {
        True => <(n, sign_bit()) | sub,
        False => n,
    }
}

func hash_mix(n: i64) -> u64 {
    <(<(<(n, -7_046_029_254_386_353_131) | wrapping_mul) | clear_sign) | into
}

func hash_chars(s: String, i: i64, acc: i64) -> i64 {
    of (<(i, <s | str_len) | lt) {
        True => {
            let code = <(<(s, i) | index) | char_to_code;
            let folded = <(<(acc, code) | xor, 1_099_511_628_211) | wrapping_mul;
            <(s, <(i, 1) | add, folded) | hash_chars
        },
        False => acc,
    }
}

spec Hash {
    func hash(self: Self) -> u64;
}
impl Hash for i64 { func hash(self: i64) -> u64 { <self | hash_mix } }
impl Hash for i8 { func hash(self: i8) -> u64 { <self | into | hash_mix } }
impl Hash for i32 { func hash(self: i32) -> u64 { <self | into | hash_mix } }
impl Hash for u64 { func hash(self: u64) -> u64 { <self | into | hash_mix } }
impl Hash for u8 { func hash(self: u8) -> u64 { <self | into | hash_mix } }
impl Hash for u32 { func hash(self: u32) -> u64 { <self | into | hash_mix } }
impl Hash for char { func hash(self: char) -> u64 { <self | char_to_code | hash_mix } }
impl Hash for String {
    func hash(self: String) -> u64 {
        <(<(<(self, 0, -3_750_763_034_362_895_579) | hash_chars) | clear_sign) | into
    }
}
impl Hash for Bool {
    func hash(self: Bool) -> u64 {
        of self {
            False => <0 | hash_mix,
            True => <1 | hash_mix,
        }
    }
}

// The character of a `String` at a position, failing at run time when the
// position is out of range; `char_at` offers that outcome to a continuation.
func index(s: String, i: i64) -> char { <(s, i) | __index }

// ── Display for anonymous data ───────────────────────────────────────────
//
// The unit, tuples and choices, up to eight components, each rendered as it
// is written: `(1, a)`, `::1(right)`, `(,)`.

impl Display for (,) {
    func fmt(self: (,)) -> String { "(,)" }
}

impl<+A: Display, +B: Display> Display for (A, B) {
    func fmt(self: (A, B)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<+A: Display, +B: Display, +C: Display> Display for (A, B, C) {
    func fmt(self: (A, B, C)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.2 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display> Display for (A, B, C, D) {
    func fmt(self: (A, B, C, D)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.2 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.3 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display, +E: Display> Display for (A, B, C, D, E) {
    func fmt(self: (A, B, C, D, E)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.2 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.3 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.4 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
    +F: Display,
> Display for (A, B, C, D, E, F) {
    func fmt(self: (A, B, C, D, E, F)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.2 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.3 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.4 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.5 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
    +F: Display,
    +G: Display,
> Display for (A, B, C, D, E, F, G) {
    func fmt(self: (A, B, C, D, E, F, G)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.2 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.3 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.4 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.5 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.6 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
    +F: Display,
    +G: Display,
    +H: Display,
> Display for (A, B, C, D, E, F, G, H) {
    func fmt(self: (A, B, C, D, E, F, G, H)) -> String {
        (<("(", <self.0 | fmt)
            | add
            | x => (x, ", ") | add
            | x => (x, <self.1 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.2 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.3 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.4 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.5 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.6 | fmt) | add
            | x => (x, ", ") | add
            | x => (x, <self.7 | fmt) | add
            | x => (x, ")") | add)
    }
}

impl<+A: Display, +B: Display> Display for (A | B) {
    func fmt(self: (A | B)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

impl<+A: Display, +B: Display, +C: Display> Display for (A | B | C) {
    func fmt(self: (A | B | C)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
            ::2(x) => (<("::2(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

impl<+A: Display, +B: Display, +C: Display, +D: Display> Display for (A | B | C | D) {
    func fmt(self: (A | B | C | D)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
            ::2(x) => (<("::2(", <x | fmt) | add | y => (y, ")") | add),
            ::3(x) => (<("::3(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
> Display for (A | B | C | D | E) {
    func fmt(self: (A | B | C | D | E)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
            ::2(x) => (<("::2(", <x | fmt) | add | y => (y, ")") | add),
            ::3(x) => (<("::3(", <x | fmt) | add | y => (y, ")") | add),
            ::4(x) => (<("::4(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
    +F: Display,
> Display for (A | B | C | D | E | F) {
    func fmt(self: (A | B | C | D | E | F)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
            ::2(x) => (<("::2(", <x | fmt) | add | y => (y, ")") | add),
            ::3(x) => (<("::3(", <x | fmt) | add | y => (y, ")") | add),
            ::4(x) => (<("::4(", <x | fmt) | add | y => (y, ")") | add),
            ::5(x) => (<("::5(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
    +F: Display,
    +G: Display,
> Display for (A | B | C | D | E | F | G) {
    func fmt(self: (A | B | C | D | E | F | G)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
            ::2(x) => (<("::2(", <x | fmt) | add | y => (y, ")") | add),
            ::3(x) => (<("::3(", <x | fmt) | add | y => (y, ")") | add),
            ::4(x) => (<("::4(", <x | fmt) | add | y => (y, ")") | add),
            ::5(x) => (<("::5(", <x | fmt) | add | y => (y, ")") | add),
            ::6(x) => (<("::6(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

impl<
    +A: Display,
    +B: Display,
    +C: Display,
    +D: Display,
    +E: Display,
    +F: Display,
    +G: Display,
    +H: Display,
> Display for (A | B | C | D | E | F | G | H) {
    func fmt(self: (A | B | C | D | E | F | G | H)) -> String {
        of self {
            ::0(x) => (<("::0(", <x | fmt) | add | y => (y, ")") | add),
            ::1(x) => (<("::1(", <x | fmt) | add | y => (y, ")") | add),
            ::2(x) => (<("::2(", <x | fmt) | add | y => (y, ")") | add),
            ::3(x) => (<("::3(", <x | fmt) | add | y => (y, ")") | add),
            ::4(x) => (<("::4(", <x | fmt) | add | y => (y, ")") | add),
            ::5(x) => (<("::5(", <x | fmt) | add | y => (y, ")") | add),
            ::6(x) => (<("::6(", <x | fmt) | add | y => (y, ")") | add),
            ::7(x) => (<("::7(", <x | fmt) | add | y => (y, ")") | add),
        }
    }
}

// ── Logic ────────────────────────────────────────────────────────────────
//
// There is no `!`: negation is an ordinary function a `Bool` flows into,
// `<b | not`.

func not(b: Bool) -> Bool {
    of b { True => False, False => True }
}
