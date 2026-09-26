// Streams — the prelude's coinductive mirror of `List` — and nested
// copatterns.
//
// `menu Stream<+T> { head: T, tail: Stream<T> }` lives in the prelude with
// `repeat`, `count_from`, `map`, and `take` beside it. Only the
// demanded branch of a menu ever runs, so an infinite stream is just a
// menu that offers itself again — and `take` is the bridge back to data:
// an infinite structure cannot print whole, so `fmt(take(s, n))` is the
// honest way to show one.

// A program's own definition shadows the prelude's: this `count_from`
// answers the first two elements directly, refining `.tail` with nested
// copatterns — the arms sharing an outer destructor group into an inner
// menu, which must again cover every item.
// `count_from` is not imported: this program declares its own below.
cite stream::Stream;
cite stream::repeat;
cite stream::take;

func count_from(n: i64) -> Stream<i64> {
    mu Stream {
        head: out <= <n | out>,
        tail: head: out <= <(n, 1) | add | out>,
        tail: tail: rest <= <(n, 2) | add | count_from | rest>,
    }
}

func double(n: i64) -> i64 { <(n, 2) | mul }

proc main | (exit: i32) / {IO} {
    let s = <10 | count_from;
    <s.head | println; // 10
    <s.tail.head | println; // 11
    <s.tail.tail.tail.head | println; // 13
    <(<(double, s) | stream::map).tail.head | println; // 22
    <(s, 3) | take | fmt | println; // "[10, 11, 12]"
    <(<7 | repeat, 2) | take | fmt | println; // "[7, 7]"
    <0 | exit>
}
