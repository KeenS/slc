// Anonymous sums. `(T1 | T2)` is an enum written without a declaration, as
// `(T1, T2)` is a record: its value names its alternative by position,
// `::0(v)` or `::1(v)`, the way `Enum::Variant(v)` names it by name.

fn describe(out: String) <- (i64 | String) {
    select (i64 | String) {
        ::0(n) => ⟨"number " + (⟨n | int_to_str) | out⟩,
        ::1(s) => ⟨"text " + s | out⟩,
    }
}

// More alternatives nest to the right, as a tuple's components do, and the
// position counts along them.
fn rank(out: String) <- (i64 | bool | String) {
    select (i64 | bool | String) {
        ::0(n) => ⟨"first " + (⟨n | int_to_str) | out⟩,
        ::1(b) => ⟨(if b { "second, yes" } else { "second, no" }) | out⟩,
        ::2(s) => ⟨"third " + s | out⟩,
    }
}

fn show(x: (i64 | String)) -> String {
    match x {
        ::0(n) => ⟨n | int_to_str,
        ::1(s) => s,
    }
}

// A command offers its outcome as one value, for a bundle of exits to take:
// the consumer of `(A | B)` is `(-A & -B)`, and the position picks the exit.
command classify(n: i64) | (outcome: (i64 | String)) {
    if n < 10 { ⟨::0(n) | outcome⟩ } else { ⟨::1("big") | outcome⟩ }
}

command main | (exit: i32) / {IO} {
    ⟨mu String { s <= ⟨::0(7) | (⟨s | describe)⟩ } | println;
    ⟨mu String { s <= ⟨::1("hi") | (⟨s | describe)⟩ } | println;
    ⟨mu String { s <= ⟨::1(true) | (⟨s | rank)⟩ } | println;
    ⟨mu String { s <= ⟨::2("last") | (⟨s | rank)⟩ } | println;
    ⟨::0(3) | show | println;
    ⟨mu String {
        s <= ⟨42 | classify | (select i64 { n => ⟨"small" | s⟩ } & select String { t => ⟨t | s⟩ })⟩
    } | println;
    ⟨0 | exit⟩
}
