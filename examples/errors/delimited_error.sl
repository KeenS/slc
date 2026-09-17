// `reset` is a boundary: a jump from inside it to a continuation captured
// outside it is refused at run time, as a jump from under a handler installed
// after the capture is.

// Hands its answer to `k` instead of returning it.
fn escape(k: -i64) -> i64 { <5 | k> }

command main | (exit: i32) / {IO} {
    let free = mu i64 { out <= <(<out | escape) | out> };
    <free | println; // 5

    // error: a continuation left the handler it was captured under
    let barred = mu i64 { out <= <(reset <out | escape) | out> };
    <barred | println;
    <0 | exit>
}
