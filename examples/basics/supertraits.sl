// `Rank` requires `Eq`. A bound `T: Rank` carries both dictionaries, so
// `eq` is `Eq`'s method and the impl of `Rank` does not declare it again.
// An impl of `Rank` is refused unless `Eq` is implemented for that type.

enum Hue { Red, Blue }

impl Eq for Hue {
    func eq(self: Hue, other: Hue) -> Bool {
        of self {
            Red => of other { Red => True, _ => False },
            Blue => of other { Blue => True, _ => False },
        }
    }
}

spec Rank: Eq {
    func place(self: Self) -> i64;
}

impl Rank for Hue {
    func place(self: Hue) -> i64 {
        of self { Red => 0, Blue => 1 }
    }
}

func same<+T: Rank>(a: T, b: T) -> Bool {
    <(a, b) | eq
}

func placed<+T: Rank>(a: T) -> i64 {
    <a | place
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

impl<+T: Rank> Rank for Pair<T> {
    func place(self: Pair<T>) -> i64 {
        <self.left | place
    }
}

proc main | (exit: i32) / {IO} {
    <(Red, Blue) | same | println;
    <(Red, Red) | same | println;
    <Blue | placed | println;
    <(Pair { left: Red, right: Blue }, Pair { left: Red, right: Red }) | same | println;
    <0 | exit>
}
