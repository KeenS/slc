// Two readings of the monotonic clock. The later one is not earlier.

proc main | (exit: i32) / {IO} {
    let start = do (<(,) | clock::now) clock::real;
    let later = do (<(,) | clock::now) clock::real;
    <(<(later, start) | ge) | println;
    <0 | exit>
}
