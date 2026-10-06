// A cut leaves the function. The other arm returns, and the caller goes on.

cite list::List;
cite list::List::*;

func first_even(xs: List<i64>, found: -i64) -> (,) {
    of xs {
        Nil => (,),
        Cons(n, rest) => of (<(n, 2) | rem) {
            0 => <n | found>,
            _ => <(rest, found) | first_even,
        },
    }
}

proc main | (exit: i32) / {IO} {
    let xs = Cons(1, Cons(2, Cons(3, Nil)));
    <mu i64 {
        out <= {
            <(xs, out) | first_even;
            <0 | out>
        },
    } | println;
    let ys = Cons(1, Cons(3, Nil));
    <mu i64 {
        out <= {
            <(ys, out) | first_even;
            <0 | out>
        },
    } | println;
    <0 | exit>
}
