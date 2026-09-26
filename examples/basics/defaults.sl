// `Eq` requires `eq`. `ne` has a default, the negation of `eq`, so an impl
// writes `eq` only. The default is the impl's own method: a generic impl
// calls `eq` through the same dictionary.

enum Hue { Red, Blue }

impl Eq for Hue {
    func eq(self: Hue, other: Hue) -> Bool {
        of self {
            Red => of other { Red => True, _ => False },
            Blue => of other { Blue => True, _ => False },
        }
    }
}

data Pair<+T> { left: T, right: T }

impl<+T: Eq> Eq for Pair<T> {
    func eq(self: Pair<T>, other: Pair<T>) -> Bool {
        of (<(self.left, other.left) | eq) {
            True => <(self.right, other.right) | eq,
            False => False,
        }
    }
}

func differ<+T: Eq>(a: T, b: T) -> Bool {
    <(a, b) | ne
}

proc main | (exit: i32) / {IO} {
    <(Red, Red) | differ | println;
    <(Red, Blue) | differ | println;
    <(Pair { left: 1, right: 2 }, Pair { left: 1, right: 2 }) | differ | println;
    <(Pair { left: 1, right: 2 }, Pair { left: 1, right: 3 }) | differ | println;
    <0 | exit>
}
