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

command main | (exit: -i32) {
    // A generic declaration: every call chooses its own `T`.
    println(id(7) + 1);
    println(str_len(id("seven")));

    // A generalized `let`: one binding, three instantiations — one through
    // an alias, since a plain name is a value form too.
    let same = fn(x) { x };
    println(same(2) * 10);
    println(same("both") + "!");
    let also = same;
    println(also(true));

    // The by-name idiom. `fresh` is a lambda, hence a value, hence
    // polymorphic — and every use runs its own capture.
    let fresh = fn(u) {
        mu { k <= {
            println("capturing");
            ⟨fn(x) { x } | k⟩
        } }
    };
    println(fresh((,))(1) + 1);
    println(str_len(fresh((,))("again")));

    ⟨0 | exit⟩
}
