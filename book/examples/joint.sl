// A joint sends the pair's first component to the first continuation.

proc main | (exit: i32) / {IO} {
    <(6, 7) | (
        fn(n: i64) -> (;) {
            <n | println;
            <0 | exit>
        }
        ; fn(m: i64) -> (;) {
            <m | println;
            <1 | exit>
        }
    )>
}
