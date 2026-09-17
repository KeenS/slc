// A program in more than one file.
//
// `mod name;` — a `mod` with no body — says the module's declarations are in
// a file of their own, and the directory tree is the module tree:
//
//   main.sl               the program, which declares `shapes` and `report`
//   shapes.sl             mod shapes      — and declares `measure`
//   shapes/measure.sl     mod shapes::measure
//   report.sl             mod report
//
// A module in a file is a module: what is `pub` is reachable by path, what
// is not stays the file's own, and `use` brings names in as it always does.
// Run the program by its root: `slc run examples/programs/multi_file/main.sl`.

mod shapes;
mod report;

use shapes::Shape;

command main | (exit: i32) / {IO} {
    <Shape::Circle(5) | report::line | println;
    <Shape::Rect(6, 7) | report::line | println;
    <0 | exit>
}
