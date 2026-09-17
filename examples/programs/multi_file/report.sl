// `mod report`: a sibling reaches `shapes` by its path from the root.

use shapes::measure::area;

fn name(s: shapes::Shape) -> String {
    match s {
        shapes::Shape::Circle(_) => "circle",
        shapes::Shape::Rect(_, _) => "rectangle",
    }
}

pub fn line(s: shapes::Shape) -> String {
    <(<s | name, " of area ") | add | x => (x, <s | area | to_string) | add
}
