// A persistent string builder as a `menu`.
//
// `string::Builder` keeps its accumulated text in the environment captured by
// its menu arms. `append` transitions to a new builder; `finish` observes one.
// Since the states are persistent, two builders can share a prefix and then
// diverge without affecting one another.

command main | (exit: i32) / {IO} {
    let start = string::new();
    let left = <(start, "left: ") | string::push;
    let left = <(left, 42) | string::push;
    let right = <(start, "right") | string::push;

    <left.finish | println;
    <right.finish | println;
    <0 | exit>
}
