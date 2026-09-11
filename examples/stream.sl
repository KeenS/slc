// An infinite stream — a recursive menu — and nested copatterns.
//
// `enum` recursion gives inductive data (lists, trees); `menu` recursion
// gives coinductive codata. A Stream answers `.head` or `.tail`, and only
// the demanded branch ever runs, so an infinite stream is just a menu that
// offers itself again.

menu Stream {
    head: i64,
    tail: Stream,
}

// Nested copatterns refine an item: the `.tail` group below is answered by
// an inner menu built from the nested arms, which must again cover every
// item — `.tail(.head(out))` alone would leave `.tail(.tail(…))` unanswered.
fn count_from(n: +i64) -> Stream {
    mu Stream {
        .head(out) <= n @ out,
        .tail(.head(out)) <= n + 1 @ out,
        .tail(.tail(rest)) <= count_from(n + 2) @ rest,
    }
}

// A stream transformed: demand drives everything, so mapping is lazy.
fn doubled(s: Stream) -> Stream {
    mu Stream {
        .head(out) <= s.head * 2 @ out,
        .tail(out) <= doubled(s.tail) @ out,
    }
}

command main | (exit: -i32) {
    let s = count_from(10);
    println(s.head);                    // 10
    println(s.tail.head);               // 11
    println(s.tail.tail.tail.head);     // 13
    println(doubled(s).tail.head);      // 22
    0 @ exit
}
