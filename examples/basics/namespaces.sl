// Modules: named scopes, flattened by resolution.
//
// A `sect` groups declarations under a path, `::` reaches into it, and `cite`
// brings one name into scope. Resolution rewrites all of it away before
// checking: every declaration becomes its qualified name, so the rest of the
// compiler works on flat names — which always contained `::`, because enum
// variants are paths already.
//
// A declaration inside a module is private unless it is `pub`: reachable by
// its own module and the modules nested in it, and nowhere else. A
// declaration in no module — everything in a single-file program, and the
// prelude — is visible everywhere.

sect geometry {
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
    func squared(n: i64) -> i64 { <(n, n) | mul }

    pub func area(s: Shape) -> i64 {
        of s {
            Circle(r) => <(3, <r | squared) | mul,
            Rect(w, h) => <(w, h) | mul,
        }
    }

    pub func origin() -> Point {
        Point { x: 0, y: 0 }
    }
}

sect physics {
    // A sibling module reaches another through its path.
    pub func weight(s: geometry::Shape) -> i64 {
        <(<s | geometry::area, 10) | mul
    }
}

// `cite` makes one name local; everything else stays qualified.
cite geometry::area;

proc main | (exit: i32) / {IO} {
    <geometry::Shape::Circle(5) | area | println;
    <geometry::Shape::Rect(6, 7) | physics::weight | println;
    of geometry::origin() {
        geometry::Point { x, y } => <(x, y) | add | println,
    };
    <0 | exit>
}
