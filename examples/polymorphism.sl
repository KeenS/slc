// Polymorphism: generic declarations, and `let` under the value restriction.
//
// A declaration may take type parameters — rigid inside their own body,
// instantiated afresh at every call. A `let` generalizes only a *value*:
// something whose evaluation ran nothing, so no two instantiations can
// disagree about anything that happened. Anything that computes stays
// monomorphic — `mu` above all, since two instantiations of one captured
// continuation is the classical unsoundness.
//
// When per-use behaviour is wanted, write it: a lambda around the `mu` is a
// value, so it generalizes — and each use re-runs the capture, which the
// print inside makes visible.

fn id<T>(x: T) -> T { x }

command main | (exit: i32) {
    // A generic declaration: every call chooses its own `T`.
    (7 | id) + 1 | println;
    "seven" | id | str_len | println;

    // A generalized `let`: one binding, three instantiations — one through
    // an alias, since a plain name is a value form too.
    let same = fn(x) { x };
    (2 | same) * 10 | println;
    ("both" | same) + "!" | println;
    let also = same;
    true | also | println;

    // The by-name idiom. `fresh` is a lambda, hence a value, hence
    // polymorphic — and every use runs its own capture.
    let fresh = fn(u) {
        mu { k <= {
            "capturing" | println;
            ⟨fn(x) { x } | k⟩
        } }
    };
    ((,) | fresh)(1) + 1 | println;
    ((,) | fresh)("again") | str_len | println;

    0 | exit⟩
}
