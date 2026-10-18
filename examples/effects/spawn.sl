// A proc with `main`'s signature. `spawn` runs it on another host thread
// and returns `(,)` before it finishes. The parent prints first. The
// process waits, so the worker's line still appears. The `7` passed to
// `exit` stays on that thread, and `main` exits `0`.

proc worker | (exit: i32) / {IO} {
    // Two hundred milliseconds, so the parent prints before this line.
    do (<200000000 | sleep::sleep) sleep::real;
    <"worker" | println;
    <7 | exit>
}

proc main | (exit: i32) / {IO} {
    let started = <worker | spawn;
    <started | println;
    <"parent" | println;
    <0 | exit>
}
