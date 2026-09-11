// Lists — an ordinary recursive enum, defined in the prelude.
//
// Nothing about List is built in: `enum List<T> { Nil, Cons(T, List<T>) }`
// is prelude source, and `length`, `map`, `append`, and `nth` are ordinary
// declarations over it. `nth` can find nothing, so it is a `command`
// offering its outcomes to continuations, like the lookup builtins.
//
// Type declarations take parameters — `List<T>`, and `List<i64>` at use —
// and so do `data`, `menu`, and `form`.

fn double(n: +i64) -> i64 { n * 2 }

fn sum(xs: List<i64>) -> i64 {
    match xs {
        Nil => 0,
        Cons(n, rest) => n + sum(rest),
    }
}

command main | (exit: -i32) {
    let xs = List::Cons(1, List::Cons(2, List::Cons(39, List::Nil)));
    println(length(xs));                    // 3
    println(sum(xs));                       // 42
    println(sum(map(double, xs)));          // 84
    println(sum(append(xs, xs)));           // 84

    // nth offers its outcomes; `defaulting` answers the miss.
    println(mu i64 { out <= nth(xs, 2, out, defaulting(0, ↓out)) });   // 39
    println(mu i64 { out <= nth(xs, 9, out, defaulting(0, ↓out)) });   // 0
    0 @ exit
}
