// One area, written with -> and with <-.

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

func area_of(out: i64) <- Shape {
    mu Shape {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul | out>,
        Rect(w, h) => <(w, h) | mul | out>,
    }
}

proc main | (exit: i32) / {IO} {
    <Shape::Circle(5) | area | println;
    <Shape::Rect(6, 7) | area_of | println;
    <0 | exit>
}
