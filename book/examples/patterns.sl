// Or-patterns and an @ binding.

enum Hue { Red, Blue, Green }

func name(h: Hue) -> String {
    of h {
        Red | Blue => "cool",
        Green => "green",
    }
}

proc main | (exit: i32) / {IO} {
    <Hue::Red | name | println;
    <Hue::Green | name | println;
    let tagged = of (1, 2) { n @ (a, _) => <(n.0, a) | add };
    <tagged | println;
    <0 | exit>
}
