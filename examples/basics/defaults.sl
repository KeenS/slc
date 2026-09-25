// `Eq` requires `eq`. `ne` has a default, the negation of `eq`, so an impl
// writes `eq` only. The default is the impl's own method: a generic impl
// calls `eq` through the same dictionary.

enum Hue { Red, Blue }

impl Eq for Hue {
    fn eq(self: Hue, other: Hue) -> Bool {
        match self {
            Red => match other { Red => True, _ => False },
            Blue => match other { Blue => True, _ => False },
        }
    }
}

data Pair<+T> { left: T, right: T }

impl<+T: Eq> Eq for Pair<T> {
    fn eq(self: Pair<T>, other: Pair<T>) -> Bool {
        match (<(self.left, other.left) | eq) {
            True => <(self.right, other.right) | eq,
            False => False,
        }
    }
}

fn differ<+T: Eq>(a: T, b: T) -> Bool {
    <(a, b) | ne
}

command main | (exit: i32) / {IO} {
    <(Red, Red) | differ | println;
    <(Red, Blue) | differ | println;
    <(Pair { left: 1, right: 2 }, Pair { left: 1, right: 2 }) | differ | println;
    <(Pair { left: 1, right: 2 }, Pair { left: 1, right: 3 }) | differ | println;
    <0 | exit>
}
