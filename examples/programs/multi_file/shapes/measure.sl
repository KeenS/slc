// `mod shapes::measure`. A name is looked for in each enclosing module, out
// to the root, so `Shape` here is the parent's `shapes::Shape`.

use Shape::*;

// Private: the file's own helper, and no part of the module's surface.
fn squared(n: i64) -> i64 {
    <(n, n) | mul
}

pub fn area(s: Shape) -> i64 {
    match s {
        Circle(r) => <(3, <r | squared) | mul,
        Rect(w, h) => <(w, h) | mul,
    }
}
