// A section keeps a helper private and publishes the rest.

sect geometry {
    pub enum Shape {
        Circle(i64),
        Rect(i64, i64),
    }

    func squared(n: i64) -> i64 { <(n, n) | mul }

    pub func area(s: Shape) -> i64 {
        of s {
            Circle(r) => <(3, <r | squared) | mul,
            Rect(w, h) => <(w, h) | mul,
        }
    }
}

cite geometry::area;

proc main | (exit: i32) / {IO} {
    <geometry::Shape::Circle(5) | area | println;
    <geometry::Shape::Rect(6, 7) | area | println;
    <0 | exit>
}
