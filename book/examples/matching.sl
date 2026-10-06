// An enum, an exhaustive of, a wildcard, and an inclusive range.

enum Shape {
    Circle(i64),
    Rect(i64, i64),
}

func area(s: Shape) -> i64 {
    of s {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul,
        Rect(w, h) => <(w, h) | mul,
    }
}

proc main | (exit: i32) / {IO} {
    <Shape::Circle(5) | area | println;
    <Shape::Rect(6, 7) | area | println;
    let n = of 2 { 0 => "zero", 1 => "one", _ => "many" };
    <n | println;
    let band = of 4 { 1..=3 => "low", 4..=6 => "mid", _ => "high" };
    <band | println;
    <0 | exit>
}
