// Streams — the prelude's coinductive mirror of `List` — and nested
// copatterns.
//
// `menu Stream<T> { head: T, tail: Stream<T> }` lives in the prelude with
// `repeat`, `count_from`, `map_stream`, and `take` beside it. Only the
// demanded branch of a menu ever runs, so an infinite stream is just a
// menu that offers itself again — and `take` is the bridge back to data:
// an infinite structure cannot print whole, so `fmt(take(s, n))` is the
// honest way to show one.

// A program's own definition shadows the prelude's: this `count_from`
// answers the first two elements directly, refining `.tail` with nested
// copatterns — the arms sharing an outer destructor group into an inner
// menu, which must again cover every item.
fn count_from(n: +i64) -> Stream<i64> {
    mu Stream {
        head: out <= n @ out,
        tail: head: out <= n + 1 @ out,
        tail: tail: rest <= count_from(n + 2) @ rest,
    }
}

fn double(n: +i64) -> i64 { n * 2 }

command main | (exit: -i32) {
    let s = count_from(10);
    println(s.head);                            // 10
    println(s.tail.head);                       // 11
    println(s.tail.tail.tail.head);             // 13
    println(map_stream(double, s).tail.head);   // 22
    println(fmt(take(s, 3)));                   // "[10, 11, 12]"
    println(fmt(take(repeat(7), 2)));           // "[7, 7]"
    0 @ exit
}
