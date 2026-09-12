// The connectives, in both polarities.
//
// There are four, and now there is one declaration for each. They split on
// two axes: whether something *chooses* (additive) or everything is in play
// at once (multiplicative), and which side holds the value.
//
//   type    declared   a value of it is              fed by
//   A ⊗ B   data       every field, at once          —  (it is the data)
//   A ⊕ B   enum       one tagged variant            —  (it is the data)
//   A ⅋ B   form       a consumer wanting all        its record   (⊗)
//   A & B   menu       a value answering one item    its request  (⊕)
//
// Three constructs cover all four. `match` takes a *named* scrutinee apart,
// on either side. `select` answers data: it builds the consumer of whatever
// shape arrives — so it also builds a form, which consumes a record. `mu`
// answers demands: with arms it builds a menu, whose requests choose.
//
// The negative connectives do not need a declaration — every positive type
// already has a dual, and `select` builds it. What `form` and `menu` add is
// a *name* for the negative side, so a signature can speak of it directly.

// ─── A ⊗ B ─── the positive product: a value carries every part.

data Pair {
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
        Pair { left, right } => (left + right) | out⟩,
    }
}

// The same connective, declared in its own right. A form's fields name what
// flows *in*, and the record they describe is what feeds it.

form Total {
    left: i64,
    right: i64,
}

fn total(out: -i64) -> Total {
    select Total {
        Total { left, right } => (left + right) | out⟩,
    }
}

// A bare product needs no declaration either; its shape is written as the type.

fn report_first(out: -i64) <- (+i64 ⊗ +String) {
    select (+i64 ⊗ +String) {
        (count, label) => count | out⟩,
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
        Red => 0 | out⟩,
        Green => 1 | out⟩,
        Blue => 2 | out⟩,
    }
}

// Declared in its own right, `&` is codata: a value that answers whichever
// item is demanded. Its dual is the sum of those demands, each carrying the
// continuation that wants the answer — `menu Config` below is exactly
// `dual(enum { Retries(-i64), Name(-String) })`, which is how this had to
// be written before the negative side could be declared.

menu Config {
    retries: i64,
    name: String,
}

fn config() -> Config {
    mu Config {
        retries <= 3 | retries⟩,
        name <= "slant" | name⟩,
    }
}

// ─── 1 and ⊥ ─── the units of the multiplicatives: the empty product, and
// its dual, the consumer that accepts it. (`0` and `⊤`, the additive units,
// have no variants to write and so no surface form.)

fn done(k: -⊥) <- unit {
    (,) | k⟩
}

command main | (exit: -i32) {
    // ⊗ : build every part, then take them apart.
    Pair { left: 2, right: 40 } | sum | println;

    // ⅋ : hand the consumer the whole product — as the dual of a declared
    // positive, and as a form declared directly. Both are the same cut.
    (mu i64 { answer <= Pair { left: 2, right: 40 } | (answer | report_sum)⟩ } | println);
    (mu i64 { answer <= Total { left: 2, right: 40 } | (answer | total)⟩ } | println);
    (mu i64 { answer <= (7, "ignored") | (answer | report_first)⟩ } | println);

    // ⊕ : build one variant, then branch on it.
    Colour::Green | name | println;

    // & : hand the consumer one variant; only its branch runs.
    (mu i64 { answer <= Colour::Green | (answer | code)⟩ } | println);

    // codata: demand one item of the menu. The other is never computed.
    config().retries | println;
    config().name | println;

    // 1 and ⊥.
    mu ⊥ { k <= (k | done) } | println;

    0 | exit⟩
}
