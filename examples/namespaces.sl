// Modules: named scopes, flattened by resolution.
//
// A `mod` groups declarations under a path, `::` reaches into it, and `use`
// brings one name into scope. Resolution rewrites all of it away before
// checking: every declaration becomes its qualified name, so the rest of the
// compiler works on flat names — which always contained `::`, because enum
// variants are paths already.
//
// A declaration inside a module is private unless it is `pub`: reachable by
// its own module and the modules nested in it, and nowhere else. A
// declaration in no module — everything in a single-file program, and the
// prelude — is visible everywhere.

mod geometry {
    pub data Point {
        x: i64,
        y: i64,
    }

    pub enum Shape {
        Circle(i64),
        Rect(i64, i64),
    }

    // Inside the module, its own names are bare — `Shape`, `Circle`.
    // Private: `squared` is the module's own business, and reaching it
    // from outside `geometry` is an error.
    fn squared(n: i64) -> i64 { n * n }

    pub fn area(s: Shape) -> i64 {
        match s {
            Circle(r) => 3 * (⟨r | squared),
            Rect(w, h) => w * h,
        }
    }

    pub fn origin() -> Point {
        Point { x: 0, y: 0 }
    }
}

mod physics {
    // A sibling module reaches another through its path.
    pub fn weight(s: geometry::Shape) -> i64 {
        (⟨s | geometry::area) * 10
    }
}

// `use` makes one name local; everything else stays qualified.
use geometry::area;

command main | (exit: i32) / {IO} {
    ⟨geometry::Shape::Circle(5) | area | println;
    ⟨geometry::Shape::Rect(6, 7) | physics::weight | println;
    match geometry::origin() {
        geometry::Point { x, y } => ⟨x + y | println,
    };
    ⟨0 | exit⟩
}
