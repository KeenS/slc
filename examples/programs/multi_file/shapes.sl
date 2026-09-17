// `mod shapes`, declared by `mod shapes;` in main.sl. The file is the
// module's body: there is no `mod shapes { … }` around it.

pub enum Shape {
    Circle(i64),
    Rect(i64, i64),
}

// A module file declares modules of its own, a directory down: this one is
// shapes/measure.sl.
pub mod measure;
