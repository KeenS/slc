// The connectives, in both polarities.
//
// A positive type is data: you build a value and take it apart with `match`.
// A negative type is a consumer: `select` builds one, and a cut hands it a
// value of the dual type. Two forms cover all four connectives — `match`
// takes any positive value apart, `select` builds any positive type's
// consumer — and what distinguishes them is how many shapes the type has: a
// product has one, a sum has one per variant.
//
//   type      a value of it is            it is consumed by
//   A ⊗ B     every part, at once         a cut against an `A ⅋ B`
//   A ⊕ B     one tagged variant          a cut against an `A & B`
//   A ⅋ B     `select` over a product     a cut with an `A ⊗ B`
//   A & B     `select` over a sum         a cut with an `A ⊕ B`

// ─── A ⊗ B ─── the positive product: a value carries every part.

struct Pair {
    left: i64,
    right: i64,
}

fn sum(p: Pair) -> i64 {
    match p {
        Pair { left, right } => left + right,
    }
}

// ─── A ⅋ B ─── its dual: one consumer that must be given every part.
// `dual(Pair)` is `-i64 ⅋ -i64`, so the arm binds both fields at once.

fn report_sum(out: -i64) <- Pair {
    select Pair {
        Pair { left, right } <= (left + right) @ out,
    }
}

// A bare product needs no declaration; its shape is written as the type.

fn report_first(out: -i64) <- (+i64 ⊗ +String) {
    select (+i64 ⊗ +String) {
        (count, label) <= count @ out,
    }
}

// ─── A ⊕ B ─── the positive sum: a value is one tagged variant.

enum Colour {
    Red,
    Green,
    Blue,
}

fn name(c: Colour) -> String {
    match c {
        Red => "red",
        Green => "green",
        Blue => "blue",
    }
}

// ─── A & B ─── its dual: one branch per variant, and the variant that
// arrives chooses exactly one of them. The others are never evaluated.

fn code(out: -i64) <- Colour {
    select Colour {
        Red <= 0 @ out,
        Green <= 1 @ out,
        Blue <= 2 @ out,
    }
}

// A `&` whose components are values rather than commands is codata: a
// provider. Its dual is a sum of *requests*, each carrying the continuation
// that wants the answer — `dual(-i64 ⊕ -String)` is `+i64 & +String`.

enum Request {
    Retries(-i64),
    Name(-String),
}

fn config() <- Request {
    select Request {
        Retries(k) <= 3 @ k,
        Name(k) <= "slant" @ k,
    }
}

// ─── 1 and ⊥ ─── the units of the multiplicatives: the empty product, and
// its dual, the consumer that accepts it. (`0` and `⊤`, the additive units,
// have no variants to write and so no surface form.)

fn done(k: -⊥) <- unit {
    () @ k
}

mu main | (exit: -i32) {
    // ⊗ : build every part, then take them apart.
    println(sum(Pair { left: 2, right: 40 }));

    // ⅋ : hand the consumer the whole product.
    println(mu ask | (answer: -i64) {
        Pair { left: 2, right: 40 } @ report_sum(answer)
    });
    println(mu ask | (answer: -i64) {
        (7, "ignored") @ report_first(answer)
    });

    // ⊕ : build one variant, then branch on it.
    println(name(Colour::Green));

    // & : hand the consumer one variant; only its branch runs.
    println(mu ask | (answer: -i64) {
        Colour::Green @ code(answer)
    });

    // codata: ask the provider for one field. The other is never computed.
    println(mu ask | (answer: -i64) {
        Request::Retries(answer) @ config
    });

    // 1 and ⊥.
    println(mu halt | (k: -⊥) { done(k) });

    0 @ exit
}
