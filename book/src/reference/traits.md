# Traits

A `spec` is a set of methods over `Self`. An `impl` supplies them for one
type. A method is a function selected by the type that flows in.

```sl
spec Show {
    func show(self: Self) -> String;
}

impl Show for Bool {
    func show(self: Bool) -> String {
        of self { True => "true", False => "false" }
    }
}

func labelled<+T: Show>(x: T) -> String {
    <x | show
}
```

The call is `<True | show`. Several parameters are one group, `<(a, b) |
eq`. The receiver is the `Self` parameter, which is the first.

## Coherence

An impl is keyed by the trait, its type arguments, and the implementing
type. `Into<i64>` and `Into<String>` for the same type are two impls.
Overlapping impls are refused. There is no impl whose `Self` is a bare type
parameter.

A method call is accepted when a concrete type has an impl, or when a type
parameter carries the bound. An unbounded type parameter does not. A trait
parameter the call never fixes is refused.

Method names are unique across traits. An impl method matches the spec
signature after `Self` and the trait arguments are substituted. An omitted
trailing row argument still matches, so an impl for `Stream<T>` covers
`Stream<T, row>`.

Impls exist for anonymous types too: a tuple, a sum, or the unit, keyed by
the connective and the width. The prelude's `Display` for tuples is that
form.

## Bounds and defaults

```sl
func largest<+T: Ord + Display>(a: T, b: T) -> String
impl<+T: Display> Display for list::List<T>
```

Bounds are joined with `+` inside one parameter. The next parameter's sign
comes after a comma. Bounds stay on positive type parameters. A trait's own
parameters do not carry bounds. A `data`, `enum`, `menu`, or `form`
declaration does not carry bounds.

A default body is copied into each impl that omits the method. A default may
call another method of the same spec. An impl may override the default. The
prelude writes `Eq.ne` as the negation of `eq`, and the integer impls
override it.

## Supertraits

```sl
spec Rank: Eq {
    func place(self: Self) -> i64;
}
```

Several parents are joined with `+`. A cycle is refused. An impl of the
child is refused until every parent is implemented for the same type. A
bound `<+T: Rank>` brings the parent methods as well. `Ord` and `Eq` in the
prelude are separate specs: a bound that needs both names both.

## Parameters and associated types

```sl
spec Into<+U> {
    func into(self: Self) -> U;
}
```

`Self` is the value that flows in. A parameter that appears in an argument
is fixed by that argument. A parameter that appears only in the result, as
`U` does, is fixed by the type the call is expected to have.

```sl
spec Walk {
    type Item;
    func step(self: Self) -> Walk::Item<Self>;
}

impl Walk for Countdown {
    type Item = i64;
    func step(self: Countdown) -> i64 { … }
}
```

Every impl binds each associated type. There is no default associated type
and no bound on an associated type. A use is `Walk::Item<T>`: trait
arguments first, the implementing type last. A pin is `<+T: Walk<Item =
i64>>`. An unfixed projection is refused.

## Dispatch

A call on a concrete type is a direct call to the impl. A call on a bounded
type parameter takes the method from a dictionary passed with the caller.
A one-method dictionary is the method. A several-method dictionary is a
tuple in declaration order. There is no runtime method table.

The programs are
[`examples/basics/traits.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/traits.sl),
[`examples/basics/defaults.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/defaults.sl),
[`examples/basics/supertraits.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/supertraits.sl),
[`examples/basics/associated.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/associated.sl),
and
[`examples/basics/into.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/into.sl).
