// Square root, absolute value, floor, and ceiling. A coordinate is a pixel
// index widened to `f64`. Flooring that coordinate and converting it back
// prints an integer.

func widen(n: i64) -> f64 {
    <n | into
}

func whole(n: f64) -> i64 {
    <n | into
}

proc main | (exit: i32) / {IO} {
    <4.0 | sqrt | println;
    <-3.25 | abs | println;
    <-1.5 | floor | println;
    <1.2 | ceil | println;
    let x = <(-1.5, <(<3 | widen, 0.0625) | mul) | add;
    <x | println;
    <(<x | floor) | whole | println;
    <(1.5, 2.5) | num::min | println;
    <0 | exit>
}
