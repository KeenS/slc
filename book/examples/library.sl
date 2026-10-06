// Lists, an ordered map, a string builder, and Option.

cite list::List;
cite list::List::*;
cite list::range;
cite list::sum;
cite list::filter;
cite list::reverse;

func even(n: i64) -> Bool {
    of (<(n, 2) | rem) {
        0 => True,
        _ => False,
    }
}

proc main | (exit: i32) / {IO} {
    let xs = <(1, 5) | range;
    <xs | println;
    <xs | sum | println;
    <(even, xs) | filter | println;
    <xs | reverse | println;
    let m = <map::empty() | x => (x, "a", 1) | map::insert | y => (y, "b", 2) | map::insert;
    <m | println;
    let b = string::new();
    let b = b.append("Hello");
    let b = <", SLC!" | b.append;
    <b.finish | println;
    <option::Option::Some(3) | x => (x, 0) | option::unwrap_or | println;
    <option::Option::None | x => (x, 0) | option::unwrap_or | println;
    <0 | exit>
}
