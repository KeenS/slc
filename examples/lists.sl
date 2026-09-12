// Lists — an ordinary recursive enum, defined in the prelude.
//
// Nothing about List is built in: `enum List<T> { Nil, Cons(T, List<T>) }`
// is prelude source, and `length`, `map`, `append`, and `nth` are ordinary
// declarations over it. `nth` can find nothing, so it is a `command`
// offering its outcomes to continuations, like the lookup builtins.
//
// Type declarations take parameters — `List<T>`, and `List<i64>` at use —
// and so do `data`, `menu`, and `form`.

fn double(n: i64) -> i64 { n * 2 }

fn sum(xs: List<i64>) -> i64 {
    match xs {
        Nil => 0,
        Cons(n, rest) => n + (rest | sum),
    }
}

command main | (exit: i32) / {IO} {
    let xs = List::Cons(1, List::Cons(2, List::Cons(39, List::Nil)));
    xs | length | println;                    // 3
    xs | sum | println;                       // 42
    (double, xs) | map | sum | println;          // 84
    (xs, xs) | append | sum | println;           // 84

    // nth offers its outcomes; `defaulting` answers the miss.
    mu i64 { out <= (xs, 2) | nth | (out & (0, out) | defaulting)⟩ } | println;   // 39
    mu i64 { out <= (xs, 9) | nth | (out & (0, out) | defaulting)⟩ } | println;   // 0
    0 | exit⟩
}
