// Into keeps a number when it is exact for the destination.

func as_i64(n: i32) -> i64 { <n | into }

func widen(n: i64) -> f64 { <n | into }

proc main | (exit: i32) / {IO} {
    <7 | as_i64 | println;
    <4 | widen | println;
    <9.0 | sqrt | println;
    <-1.5 | abs | println;
    <1.2 | floor | println;
    <1.2 | ceil | println;
    <0 | exit>
}
