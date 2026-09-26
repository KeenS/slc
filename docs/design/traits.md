Part of the [language design](../../DESIGN.md).

## Traits

Impls dispatch on either side of the mirror: a `menu` or a `form` carries
one exactly as a `data` or an `enum` does — `impl Describe for Config`
with the method demanding `self.retries` — including bounded impls for
generic menus.

A bounded impl — `impl<+T: Display> Display for List<T>` — keys by the
declaration's name and covers every instantiation; its dictionary is
**constructed** at each use, the impl's global applied to one dictionary per
bound, read off the use's type arguments and built recursively:
`fmt` over a `List<List<i64>>` passes
`__dict_Display_List(__dict_Display_List(__dict_Display_i64))`. A trait with
several methods stores them as a tuple. A bounded impl of such a trait
applies the element's dictionaries to each method, then builds that tuple:
a tuple of functions is not itself a function.

A `spec` names operations over an implicit `Self`; an `impl` gives them for a
type; a bound `<T: Show>` lets a generic use them. A method is a free function
overloaded on its first argument's type — `<x | show`, never `x.show()`:

```sl
spec Show { func show(self: Self) -> String; }
impl Show for i64  { func show(self: i64)  -> String { <self | int_to_str } }
impl Show for Bool { func show(self: Bool) -> String { of self { True => "t", _ => "f" } } }

func labelled<+T: Show>(x: T) -> String { (<("= ", <x | show) | add) }
```

The checker makes dispatch total: coherence allows one `impl` per trait and
type, and a method call is accepted only when the type has an impl — a ground
type directly, a bounded type parameter through its bound, and an unbounded
one not at all.

Dispatch is then resolved entirely at compile time, with no runtime method
value. A call on a concrete type compiles to a direct call to its impl. A call
on a bound type parameter `<T: Show>` projects the method from a *dictionary* —
the trait's impls for `T`, passed to the bounded function as a hidden
argument; the function forwards it to any bounded call it makes, so the impl
is chosen once, by whoever supplied the concrete type. A single-method trait's
dictionary is just its impl.

An impl may be for an anonymous type too — a tuple, a choice or the unit. It
keys by its connective and width, and its components stand where a
declaration's type arguments do, so each bound is read off the component in
its position: `impl<+A: Display, +B: Display> Display for (A, B)`.

A method of several parameters takes them as one group, as any function
does, and `Self` is read off the components its parameters give that type.
Each component is checked against its parameter, and an integer literal
takes its width from the others, so with
`spec Combine { func combine(self: Self, other: Self) -> Self; }`,
`<(1, x) | combine` for `x: i32` is `i32`'s `combine`.

A parameter may be bound by several traits, joined by `+`:

```
func largest<+T: Ord + Display>(a: T, b: T) -> String {
    of (<(a, b) | gt) { True => <a | fmt, False => <b | fmt }
}
```

Each bound is its own dictionary, passed in the order written, so a second
bound costs what a second bounded parameter would. An impl is bound the same
way, `impl<+T: Ord + Display> Show for Pair<T>`. The `+` can only join inside
the bounds — the next parameter's sign comes after a `,` — and a second `:`
is refused with the spelling that was meant.

A trait may take type parameters, written as any declaration's are. `Self`
stays the value that flows in. The parameters are fixed by the types the
call meets: an argument that has one, or — when the parameter appears only
in what the call produces — the type that result is expected to have.

```sl
spec Into<+U> { func into(self: Self) -> U; }
impl Into<i64> for Wrap { func into(self: Wrap) -> i64 { … } }
impl Into<String> for Wrap { func into(self: Wrap) -> String { … } }

func number(w: Wrap) -> i64 { <w | into }
```

`number` returns `i64`, so `<w | into` is `Into<i64>`. The same call in a
function that returns `String` is the other impl. A call that nothing
constrains has no destination and is refused. One impl is allowed per
trait, its arguments, and the implementing type, so the two impls above
are distinct. A bound carries the arguments, `<+T: Into<String>>`, and is
its own dictionary. An impl may quantify too: `impl<+T> Into<T> for Id<T>`
covers every `Id`.
[`examples/basics/into.sl`](../../examples/basics/into.sl) runs it.

A method may end in a body. An impl that omits the method gets that body;
one that writes the method overrides it. The copy is checked as the impl's
own method, so a call in it dispatches by the rules above. `Eq`'s `ne` is
that default, the negation of `eq`, and the prelude's integer impls override
it. A default may call another method of its trait. It may not call the
method it defines.
[`examples/basics/defaults.sl`](../../examples/basics/defaults.sl) writes `eq` only.

A trait may require other traits, written `spec Rank: Eq`, and several
parents join with `+`. An impl of the child is refused unless every parent
is implemented for the same type. A bound `T: Rank` is still one bound where
it is written. It carries one dictionary per trait, parents before the
child, and `<x | eq` under that bound is `Eq`'s method: the child does not
declare `eq` again, because a method name belongs to one trait. A trait that
is its own parent is refused.
[`examples/basics/supertraits.sl`](../../examples/basics/supertraits.sl) uses both
methods from `T: Rank`.

An associated type is one type the impl chooses, not a family of
destinations. A family stays a trait parameter, which is what `Into<U>` is,
and `Add` returns `Self`.

```sl
spec Walk {
    type Item;
    func next(self: Self) -> Item;
}

impl Walk for Countdown {
    type Item = i64;
    func next(self: Countdown) -> i64 { … }
}
```

`Item` in the trait's signatures stands for the impl's type. At a use the
projection is `Walk::Item<Countdown>`: the trait's own arguments come first
and the implementing type is last, so it is `i64`. Inside
`func echo<+T: Walk>(x: T) -> Walk::Item<T>` the two mentions are one type,
and a call with a `Countdown` returns that impl's `i64`. A bound may pin
the projection, `<+T: Walk<Item = i64>>`. The dictionary is still `Walk`'s;
at the call, where `T` is known, `Walk::Item<T>` must be `i64`. A projection
nothing fixes — no impl, and no bound — is refused.
[`examples/basics/associated.sl`](../../examples/basics/associated.sl) runs the
three.

A method may be a `proc`, taking continuations like any other; the
dispatch is unchanged. Method names are unique across traits, and bounds are
on positive type parameters.
