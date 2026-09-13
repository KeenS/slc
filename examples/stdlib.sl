// The library has two layers, and this program draws on the second.
//
// The prelude is what every program sees unasked: the logical units, the
// `IO` effect, and `Display` with `fmt`/`to_string`. Everything else is a
// stdlib module — `list`, `option`, `result`, `num`, `stream`, `seq`,
// `lazy`, `trace` — reached by its path, `list::length`, or brought in bare
// with `use`. A module's declarations are private unless it marks them
// `pub`, so what a module offers is exactly what it says it offers.

use list::List;          // the type, for signatures
use list::List::*;       // its variants, bare: `Nil`, `Cons`
use list::length;        // one function, bare
use option::*;           // every `pub` member of `option`, bare: `Option`, `unwrap_or`
use option::Option::*;

fn first(xs: List<i64>) -> Option<i64> {
    match xs {
        Nil => None,
        Cons(h, _) => Some(h),
    }
}

command main | (exit: i32) / {IO} {
    let xs = Cons(3, Cons(1, Cons(2, Nil)));
    xs | length | println;                              // 3
    (xs | first, 0) | unwrap_or | println;              // 3
    (Nil | first, 0) | unwrap_or | println;             // 0

    // Reached by path, nothing imported: the module's name is the prefix.
    (3, 7) | num::min | println;                        // 3
    (1 | stream::count_from, 3) | stream::take | fmt | println;   // "[1, 2, 3]"

    // `fmt` on a list is the prelude's `Display` at the stdlib's `List`:
    // the impl lives with the type, in `list`, and is found from here.
    xs | fmt | println;                                 // "[3, 1, 2]"
    0 | exit⟩
}
