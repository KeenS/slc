// Projection: `.i` reads a tuple component, `.field` reads a record field.
//
// A product is right-nested with its last component bare, so projection
// walks the spine to the i-th element — resolved from the value's type, so
// `t.2` and `p.z` know which component they name.

data Point { x: i64, y: i64, z: i64 }

fn manhattan(p: Point) -> i64 {
    p.x + p.y + p.z
}

command main | (exit: i32) / {IO} {
    let t = (10, 20, 30);
    t.0 + t.1 + t.2 | println;            // 60
    Point { x: 1, y: 2, z: 3 } | manhattan | println;   // 6
    0 | exit⟩
}
