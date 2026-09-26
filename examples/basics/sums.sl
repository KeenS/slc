// Anonymous sums. `(T1 | T2)` is an enum written without a declaration, as
// `(T1, T2)` is a record: its value names its alternative by position,
// `::0(v)` or `::1(v)`, the way `Enum::Variant(v)` names it by name.

func describe(out: String) <- (i64 | String) {
    mu (i64 | String) {
        ::0(n) => <("number ", <n | int_to_str) | add | out>,
        ::1(s) => <("text ", s) | add | out>,
    }
}

// More alternatives nest to the right, as a tuple's components do, and the
// position counts along them.
func rank(out: String) <- (i64 | Bool | String) {
    mu (i64 | Bool | String) {
        ::0(n) => <("first ", <n | int_to_str) | add | out>,
        ::1(b) => <(of b { True => "second, yes", False => "second, no" }) | out>,
        ::2(s) => <("third ", s) | add | out>,
    }
}

func show(x: (i64 | String)) -> String {
    of x {
        ::0(n) => <n | int_to_str,
        ::1(s) => s,
    }
}

// A command offers its outcome as one value, for a bundle of exits to take:
// the consumer of `(A | B)` is `(-A & -B)`, and the position picks the exit.
proc classify(n: i64) | (outcome: (i64 | String)) {
    of (<(n, 10) | lt) { True => <::0(n) | outcome>, False => <::1("big") | outcome> }
}

// An alternative flows into a function consuming its sum, and the chain
// carries on with what that function hands its continuation.
proc main | (exit: i32) / {IO} {
    <::0(7) | describe | println;
    <::1("hi") | describe | println;
    <::1(True) | rank | println;
    <::2("last") | rank | println;
    <::0(3) | show | println;
    <mu String {
        s <= <42 | classify | (mu i64 { n => <"small" | s> } & mu String { t => <t | s> })>,
    } | println;
    <0 | exit>
}
