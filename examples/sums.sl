// Anonymous sums. `(T1 | T2)` is an enum of two alternatives written without
// a declaration, as `(T1, T2)` is a record of two fields. A value fills one
// slot, and a `|` stands for each other: `(7 |)`, `(| "text")`.

fn describe(out: String) <- (i64 | String) {
    select (i64 | String) {
        (n |) => "number " + (n | int_to_str) | out⟩,
        (| s) => "text " + s | out⟩,
    }
}

// More alternatives nest to the right, as a tuple's components do, so
// `(| | s)` is `(| (| s))`.
fn rank(out: String) <- (i64 | bool | String) {
    select (i64 | bool | String) {
        (n | |) => "first " + (n | int_to_str) | out⟩,
        (| b |) => (if b { "second, yes" } else { "second, no" }) | out⟩,
        (| | s) => "third " + s | out⟩,
    }
}

// An arm may stop at the rest: `(| rest)` binds a `(bool | String)`.
fn head_or_rest(out: String) <- (i64 | bool | String) {
    select (i64 | bool | String) {
        (_ |) => "head" | out⟩,
        (| rest) => "rest" | out⟩,
    }
}

fn show(x: (i64 | String)) -> String {
    match x {
        (n |) => n | int_to_str,
        (| s) => s,
    }
}

// A command offers its outcome as one value, for a bundle of exits to take:
// dual(A ⊕ B) is -A & -B, so the alternative picks the exit.
command classify(n: i64) | (outcome: (i64 | String)) {
    if n < 10 { (n |) | outcome⟩ } else { (| "big") | outcome⟩ }
}

command main | (exit: i32) / {IO} {
    mu String { s <= (7 |) | (s | describe)⟩ } | println;
    mu String { s <= (| "hi") | (s | describe)⟩ } | println;
    mu String { s <= (| true |) | (s | rank)⟩ } | println;
    mu String { s <= (|| "last") | (s | rank)⟩ } | println;
    mu String { s <= (| false |) | (s | head_or_rest)⟩ } | println;
    (3 |) | show | println;
    mu String {
        s <= 42 | classify | (select i64 { n => "small" | s⟩ } & select String { t => t | s⟩ })⟩
    } | println;
    0 | exit⟩
}
