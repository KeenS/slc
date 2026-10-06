# Traits

A `spec` names operations over an implicit `Self`. An `impl` gives them for
a type. A bound `<+T: Show>` lets a generic function call them. The method
is a function, and the value flows in: `<x | show`. Dispatch is resolved
while the program is checked.

```sl
{{#include ../../examples/traits.sl}}
```

```text
n=7
2
1
```

## A method

```sl
spec Show {
    func show(self: Self) -> String;
}

impl Show for i64 {
    func show(self: i64) -> String { <self | int_to_str }
}
```

The parameter is named `self` by convention. The impl repeats the signature
with `Self` replaced by `i64`. `<7 | show` selects that impl from the type
of `7`.

A generic function states the bound and calls the method the same way:

```sl
func labelled<+T: Show>(label: String, x: T) -> String {
    <(label, <x | show) | add
}
```

`<("n=", 7) | labelled` fixes `T` to `i64`. Several bounds on one parameter
are joined with `+`, as in `<+T: Ord + Display>`. Each bound is checked on
its own. A call that never fixes a trait parameter is refused.

One impl is allowed for a given trait, its arguments, and the implementing
type. A second `impl Show for i64` is refused.

## A default method

A method may have a body in the spec. An impl that omits it receives that
body, with `Self` substituted.

```sl
spec Kind {
    func code(self: Self) -> i64;
    func labelled_code(self: Self) -> String { <self | code | int_to_str }
}
```

`impl Kind for Hue` writes `code` only. `<Hue::Red | labelled_code` runs the
default, which calls `code` and prints `1`. A default may call another method
of the same spec. The prelude's `Eq.ne` is the negation of `eq`, and an impl
may still override it.

## Parameters and associated types

A spec may take type parameters. `Self` stays the value that flows in. `Into`
is the prelude's example: `Into<+U>` converts `Self` into `U`, and the type
expected of the call chooses `U`.

```sl
func as_i64(n: i32) -> i64 { <n | into }
```

`<7 | as_i64` prints `7`. The conversion keeps the number when it is exact
for the destination. A value that does not fit is an arithmetic overflow.
The widths and the float operations are the next program.

```sl
{{#include ../../examples/into.sl}}
```

```text
7
4
3
1.5
1
2
```

`sqrt`, `abs`, `floor`, and `ceil` are methods of `f32` and `f64`. `num::abs`
is a separate function on `i64`.

An associated type is chosen by the impl:

```sl
spec Walk {
    type Item;
    func step(self: Self) -> Walk::Item<Self>;
}
```

A use writes `Walk::Item<T>`: the trait's arguments first, when it has any,
and the implementing type last. A bound can pin it, `<+T: Walk<Item = i64>>`.
The worked program is
[`examples/basics/associated.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/associated.sl).
A supertrait is `spec Rank: Eq`, and an impl of `Rank` is accepted when `Eq`
is implemented for the same type. See
[`examples/basics/supertraits.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/supertraits.sl).

The [traits reference](../reference/traits.md) records the rules for
dictionaries, overlapping impls, and associated types.
