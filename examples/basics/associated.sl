// `Walk::Item` is the type the impl chooses. A concrete use is that type.
// Under `T: Walk` it is one type for `T`, and the call returns the impl's
// item. A bound may pin it: `<+T: Walk<Item = i64>>` still passes the
// `Walk` dictionary, and the call checks the pin.

enum Countdown { Step(i64), Done }

spec Walk {
    type Item;
    func next(self: Self) -> Item;
}

impl Walk for Countdown {
    type Item = i64;
    func next(self: Countdown) -> i64 {
        of self { Step(n) => n, Done => 0 }
    }
}

enum Words { One, Two }

impl Walk for Words {
    type Item = String;
    func next(self: Words) -> String {
        of self { One => "one", Two => "two" }
    }
}

data Id<+T> { value: T }

impl<+T> Walk for Id<T> {
    type Item = T;
    func next(self: Id<T>) -> T {
        self.value
    }
}

func echo<+T: Walk>(x: T) -> Walk::Item<T> {
    <x | next
}

func number<+T: Walk<Item = i64>>(x: T) -> i64 {
    <x | next
}

func passed<+T: Walk<Item = i64>>(x: T) -> i64 {
    <x | number
}

proc main | (exit: i32) / {IO} {
    <Step(3) | echo | println;
    <One | echo | println;
    <Id { value: 8 } | echo | println;
    <Step(4) | number | println;
    <Done | passed | println;
    <0 | exit>
}
