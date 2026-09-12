// μ̃ — the value abstraction.
//
// `μ̃x. c` is the co-term that receives a value, names it `x`, and runs the
// command `c`. It is the dual of `μ`: `μk. c` captures the continuation it is
// cut against, `μ̃x. c` captures the value, and the cut between the two halves
// is substitution:
//
//    v ∥ μ̃x. c ⟩  →  c[v/x]
//
// Every binder in the language is one. This program writes the same co-term
// four ways, and passes one around as the consumer it is.

// A `mu` that hands its result to whatever consumer it is given — including
// one built below.
command twice(x: i64) | (k: i64) {
    (x * 2) | k⟩
}

command main | (exit: i32) / {IO} {
    // 1. `let` is a μ̃. `let x = v; rest` lowers to
    //    `μlet.  v ∥ μ̃x.  rest ∥ let ⟩ ⟩`: the value is cut against a
    //    binder, and the rest of the block is what that binder runs.
    let doubled = 21 * 2;
    doubled | println;

    // 2. The same co-term, written directly. `select` builds the consumer of
    //    a positive type, and an atom is the degenerate product — one shape,
    //    one component — so its one arm binds the whole value with a plain
    //    name. This is `μ̃n. println(n) ∥ … ⟩`, spelled in the surface.
    let show = select i64 { n => n | println };

    // Cutting a value against it substitutes: `n` is `42` inside the arm.
    doubled | show⟩;

    // 3. A binder that ignores its value is the same co-term with `_` for a
    //    name — and it is what sequencing lowers to. `e₁; e₂` runs `e₁`, binds
    //    its value to a name nobody mentions, and runs `e₂`; the two lines
    //    below are that, taken apart.
    let discard = select String { _ => "the value was consumed" | println };
    "thrown away" | discard⟩;

    // 4. More than one binder is the multiplicative μ̃. A product has one
    //    shape too, but several components, and they arrive together in one
    //    command sharing its context — which is what `⅋` means.
    let report = select (i64 ⊗ i64) { (left, right) => left + right | println };
    (10, 7) | report⟩;

    // 5. A μ̃ is an ordinary consumer, so it goes wherever one is wanted: this
    //    one is the continuation `twice` activates.
    50 | twice | select i64 { n => n | println }⟩;

    0 | exit⟩
}

// The labelled μ̃ — `μ̃[ L(x…). c … ]`, one branch per variant of an `enum` —
// is the additive form of the same idea, and `select.sl` writes that one.
