// `option::map` and `option::and_then`. The function comes first. `None`
// stays `None`, and `and_then` keeps the `Option` the function returns.

cite option::Option;
cite option::Option::*;

func show<+T: Display>(o: Option<T>) -> String {
    of o {
        None => "none",
        Some(x) => <x | fmt,
    }
}

func next(n: i64) -> i64 {
    <(n, 1) | add
}

func kept(n: i64) -> Option<i64> {
    of (<(n, 0) | gt) {
        True => Some(n),
        False => None,
    }
}

proc main | (exit: i32) / {IO} {
    <(next, Some(2)) | option::map | show | println;
    <(next, None) | option::map | show | println;
    <(kept, Some(4)) | option::and_then | show | println;
    <(kept, Some(0)) | option::and_then | show | println;
    <(kept, None) | option::and_then | show | println;
    <0 | exit>
}
