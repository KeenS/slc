// Modules: named scopes, flattened by resolution.
//
// A `mod` groups declarations under a path, `::` reaches into it, and `use`
// brings one name into scope. Resolution rewrites all of it away before
// checking: every declaration becomes its qualified name, so the rest of the
// compiler works on flat names — which always contained `::`, because enum
// variants are paths already.

mod geometry {
    data Point {
        x: i64,
        y: i64,
    }

    enum Shape {
        Circle(i64),
        Rect(i64, i64),
    }

    // Inside the module, its own names are bare — `Shape`, `Circle`.
    fn area(s: Shape) -> i64 {
        match s {
            Circle(r) => 3 * r * r,
            Rect(w, h) => w * h,
        }
    }

    fn origin() -> Point {
        Point { x: 0, y: 0 }
    }
}

mod physics {
    // A sibling module reaches another through its path.
    fn weight(s: geometry::Shape) -> i64 {
        (s | geometry::area) * 10
    }
}

// `use` makes one name local; everything else stays qualified.
use geometry::area;

command main | (exit: i32) / {IO} {
    geometry::Shape::Circle(5) | area | println;
    geometry::Shape::Rect(6, 7) | physics::weight | println;
    match geometry::origin() {
        geometry::Point { x, y } => x + y | println,
    };
    0 | exit⟩
}
