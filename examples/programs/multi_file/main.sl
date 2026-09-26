// A program in more than one file.
//
// `sect name;` — a `sect` with no body — says the module's declarations are in
// a file of their own, and the directory tree is the module tree:
//
//   main.sl               the program, which declares `shapes` and `report`
//   shapes.sl             sect shapes      — and declares `measure`
//   shapes/measure.sl     sect shapes::measure
//   report.sl             sect report
//
// A module in a file is a module: what is `pub` is reachable by path, what
// is not stays the file's own, and `cite` brings names in as it always does.
// Run the program by its root: `slc run examples/programs/multi_file/main.sl`.

sect shapes;
sect report;

cite shapes::Shape;

proc main | (exit: i32) / {IO} {
    <Shape::Circle(5) | report::line | println;
    <Shape::Rect(6, 7) | report::line | println;
    <0 | exit>
}
