// The library has two layers, and this program draws on the second.
//
// The prelude is what every program sees unasked: the logical units, the
// `IO` effect, and `Display` with `fmt`/`to_string`. Everything else is a
// stdlib module — `list`, `option`, `result`, `num`, `stream`, `seq`,
// `lazy`, `trace` — reached by its path, `list::length`, or brought in bare
// with `cite`. A module's declarations are private unless it marks them
// `pub`, so what a module offers is exactly what it says it offers.

cite list::List; // the type, for signatures
cite list::List::*; // its variants, bare: `Nil`, `Cons`
cite list::length; // one function, bare
cite option::*; // every `pub` member of `option`, bare: `Option`, `unwrap_or`
cite option::Option::*;

func first(xs: List<i64>) -> Option<i64> {
    of xs {
        Nil => None,
        Cons(h, _) => Some(h),
    }
}

proc main | (exit: i32) / {IO} {
    let xs = Cons(3, Cons(1, Cons(2, Nil)));
    <xs | length | println; // 3
    <(<xs | first, 0) | unwrap_or | println; // 3
    <(<Nil | first, 0) | unwrap_or | println; // 0

    // Reached by path, nothing imported: the module's name is the prefix.
    <(3, 7) | num::min | println; // 3
    <(18, -24) | num::gcd | println; // 6
    <(18, -24) | num::lcm | println; // 72
    <-7 | num::signum | println; // -1
    <-4 | num::is_even | println; // true
    <(17, 5) | num::div_rem | fmt | println; // (3, 2)
    <(<1 | stream::count_from, 3) | stream::take | fmt | println; // "[1, 2, 3]"

    // `fmt` on a list is the prelude's `Display` at the stdlib's `List`:
    // the impl lives with the type, in `list`, and is found from here.
    <xs | fmt | println; // "[3, 1, 2]"
    <0 | exit>
}
