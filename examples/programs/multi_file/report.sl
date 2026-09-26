// `sect report`: a sibling reaches `shapes` by its path from the root.

cite shapes::measure::area;

func name(s: shapes::Shape) -> String {
    of s {
        shapes::Shape::Circle(_) => "circle",
        shapes::Shape::Rect(_, _) => "rectangle",
    }
}

pub func line(s: shapes::Shape) -> String {
    <(<s | name, " of area ") | add | x => (x, <s | area | to_string) | add
}
