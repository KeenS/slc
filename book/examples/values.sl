// Literals, tuples, records, strings, and comparisons.

data Point { x: i64, y: i64 }

proc main | (exit: i32) / {IO} {
    <(2, 3) | add | println;
    <("Hello, ", "SLC") | add | println;
    <True | not | println;
    let (a, b) = (10, 20);
    <(a, b) | add | println;
    let p = Point { x: 1, y: 2 };
    <(p.x, p.y) | add | println;
    <'A' | println;
    <1.5 | println;
    let text = <("Hello, ", "SLC") | add;
    <(text, 7) | index | println;
    <(text, 7, 10) | substring | println;
    <(3, 5) | lt | println;
    <0 | exit>
}
