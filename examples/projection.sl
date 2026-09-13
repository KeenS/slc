// Projection: `.i` reads a tuple component, `.field` reads a record field.
//
// Projection is resolved against the value's type, so `t.2` and `p.z` know
// which component they name. Nesting is significant: `(10, (20, 30))` has two
// components, and its `.1` is `(20, 30)`.

data Point { x: i64, y: i64, z: i64 }

fn manhattan(p: Point) -> i64 {
    (⟨(p.x, p.y) | add | x => (x, p.z) | add)
}

command main | (exit: i32) / {IO} {
    let t = (10, 20, 30);
    ⟨(t.0, t.1) | add | x => (x, t.2) | add | println;            // 60
    ⟨Point { x: 1, y: 2, z: 3 } | manhattan | println;   // 6
    ⟨0 | exit⟩
}
