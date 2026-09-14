// Polymorphism: generic declarations, and `let` under the value restriction.
//
// A declaration may take type parameters — rigid inside their own body,
// instantiated afresh at every call. A `let` generalizes only a *value*:
// something whose evaluation ran nothing, so no two instantiations can
// disagree about anything that happened. Anything that computes stays
// monomorphic — `mu` above all, since two instantiations of one captured
// continuation is the classical unsoundness. A lambda's parameter must have a
// known polarity, so a lambda nothing pins down does not generalize: a
// polymorphic function is a declaration.
//
// When per-use behaviour is wanted, write it: a lambda around the `mu` is a
// value, and each use re-runs the capture, which the print inside makes
// visible.

enum Maybe<+T> { Nothing, Just(T) }

fn id<+T>(x: T) -> T { x }

fn or_else<+T>(m: Maybe<T>, fallback: T) -> T {
    match m { Maybe::Just(x) => x, Maybe::Nothing => fallback }
}

command main | (exit: i32) / {IO} {
    // A generic declaration: every call chooses its own `T`.
    <(<7 | id, 1) | add | println;
    <"seven" | id | str_len | println;

    // A generalized `let`: one binding, three instantiations — one through
    // an alias, since a plain name is a value form too.
    let nothing = Maybe::Nothing;
    <(<(nothing, 2) | or_else, 10) | mul | println;
    <(<(nothing, "both") | or_else, "!") | add | println;
    let also = nothing;
    <(also, True) | or_else | println;

    // The by-name idiom. `fresh` is a lambda, hence a value — and every use
    // runs its own capture.
    let fresh = fn {
        mu { k <= {
            <"capturing" | println;
            <fn(x: i64) { x } | k>
        } }
    };
    <((<(,) | fresh)(1), 1) | add | println;
    <((<(,) | fresh)(4), 1) | add | println;

    <0 | exit>
}
