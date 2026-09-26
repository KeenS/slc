// Several traits and several effects in one function.
//
// A function may be bound by more than one trait — on separate type
// parameters, or several on one, joined by `+` — and may perform more than
// one effect, listing them all in its row. Traits are discharged at the
// call by the argument types; effects are discharged by handlers, one per
// effect, nested.

// ── Several traits ──────────────────────────────────────────────────────

spec Show { func show(self: Self) -> String; }
spec Width { func width(self: Self) -> i64; }

impl Show for i64 { func show(self: i64) -> String { <self | int_to_str } }
impl Show for Bool { func show(self: Bool) -> String { of self { True => "yes", False => "no" } } }
impl Show for String { func show(self: String) -> String { self } }
impl Width for i64 { func width(self: i64) -> i64 { <self | int_to_str | str_len } }

// Two type parameters, two bounds: `show` resolves on each argument's type.
func pair<+A: Show, +B: Show>(a: A, b: B) -> String {
    <(<a | show, ", ") | add | x => (x, <b | show) | add
}

// Bounds from two different traits at once.
func show_with_width<+T: Show, +N: Width>(label: T, n: N) -> String {
    <(<label | show, " (")
        | add
        | x => (x, <n | width | int_to_str) | add
        | x => (x, " digits)") | add
}

// Two bounds on one parameter: `n` is both shown and measured.
func framed<+N: Show + Width>(n: N) -> String {
    <(<n | show, " is ") | add | x => (x, <n | width | int_to_str) | add | x => (x, " wide") | add
}

// ── Several effects ─────────────────────────────────────────────────────

hook Exn { func fail(message: String) -> i64; }
hook Reader { func config() -> i64; }

// The row lists every effect the body may perform.
func scale(x: i64) -> i64 / {Exn, Reader} {
    of (<(x, 0) | eq) {
        True => <"cannot scale zero" | fail,
        False => <(x, config()) | mul,
    }
}

proc main | (exit: i32) / {IO} {
    // multiple traits, resolved per argument type
    <(42, True) | pair | println;
    <("n", 1234) | show_with_width | println;
    <1234 | framed | println;

    // multiple effects, discharged by nested handlers — the inner handles
    // Exn, the outer Reader. Either order works; each handler answers its
    // own operations.
    let ok = do (do (<5 | scale) {
        fail(m) => -1,
    }) {
        config(): resume => <10 | resume,
    };
    <ok | println; // 5 * 10 = 50

    let bad = do (do (<0 | scale) {
        fail(m) => -1,
    }) {
        config(): resume => <10 | resume,
    };
    <bad | println; // failed → -1

    <0 | exit>
}
