// `Rank` requires `Eq`. A bound `T: Rank` carries both dictionaries, so
// `eq` is `Eq`'s method and the impl of `Rank` does not declare it again.
// An impl of `Rank` is refused unless `Eq` is implemented for that type.

enum Hue { Red, Blue }

impl Eq for Hue {
    fn eq(self: Hue, other: Hue) -> Bool {
        match self {
            Red => match other { Red => True, _ => False },
            Blue => match other { Blue => True, _ => False },
        }
    }
}

trait Rank: Eq {
    fn place(self: Self) -> i64;
}

impl Rank for Hue {
    fn place(self: Hue) -> i64 {
        match self { Red => 0, Blue => 1 }
    }
}

fn same<+T: Rank>(a: T, b: T) -> Bool {
    <(a, b) | eq
}

fn placed<+T: Rank>(a: T) -> i64 {
    <a | place
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

impl<+T: Rank> Rank for Pair<T> {
    fn place(self: Pair<T>) -> i64 {
        <self.left | place
    }
}

command main | (exit: i32) / {IO} {
    <(Red, Blue) | same | println;
    <(Red, Red) | same | println;
    <Blue | placed | println;
    <(Pair { left: Red, right: Blue }, Pair { left: Red, right: Red }) | same | println;
    <0 | exit>
}
