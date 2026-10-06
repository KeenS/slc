// A proc offers one exit per outcome. Returning functions make it yield.

proc choose(n: i64) | (nonneg: (-i64) & neg: (-i64)) {
    of (<(n, 0) | lt) {
        True => <n | neg>,
        False => <n | nonneg>,
    }
}

proc main | (exit: i32) / {IO} {
    <4 | choose | (fn(n: i64) { n } & fn(n: i64) { 0 }) | println;
    <-3 | choose | (fn(n: i64) { 0 } & fn(n: i64) { n }) | println;
    <0 | exit>
}
