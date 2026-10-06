// A spec, an impl, a bound, and a default method.

spec Show {
    func show(self: Self) -> String;
}

impl Show for i64 {
    func show(self: i64) -> String { <self | int_to_str }
}

func labelled<+T: Show>(label: String, x: T) -> String {
    <(label, <x | show) | add
}

spec Kind {
    func code(self: Self) -> i64;
    func labelled_code(self: Self) -> String { <self | code | int_to_str }
}

enum Hue { Red, Blue }

impl Kind for Hue {
    func code(self: Hue) -> i64 {
        of self { Red => 1, Blue => 2 }
    }
}

proc main | (exit: i32) / {IO} {
    <("n=", 7) | labelled | println;
    <Hue::Blue | code | println;
    <Hue::Red | labelled_code | println;
    <0 | exit>
}
