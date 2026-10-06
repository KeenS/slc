# Data and matching

An `enum` is a value that is one variant. `of` takes it apart. Every variant
appears in some arm, each variant appears once, and the order of the arms
does not matter.

```sl
{{#include ../../examples/matching.sl}}
```

```text
75
42
many
mid
```

`Shape::Circle(5)` builds a circle. `area` matches it and multiplies. The
circle arm uses a chain binder, `x =>`, to name the product `<(3, r) | mul`
before multiplying by the radius again. The area of a circle of radius 5,
with 3 standing in for π, is 75. The rectangle is `6 * 7`, which is 42.

## Arms

An arm is a pattern, then `=>`, then the expression that pattern produces.
There is no test between the pattern and the arrow. A test on a bound name
is another `of` inside the arm, which is what `even` does in the
[library](library.md) chapter.

A wildcard `_` covers whatever the earlier arms left. In `of 2 { 0 => "zero",
1 => "one", _ => "many" }` the wildcard is the remaining integers. A wildcard
that sits beside an arm for the same value is refused: coverage is exact.

An inclusive range is `start..=end`. It takes the numeric type of the
scrutinee. `4` falls in `4..=6`.

## Or-patterns and `@`

```sl
{{#include ../../examples/patterns.sl}}
```

```text
cool
green
2
```

`Red | Blue` is one arm covering two variants. `@` binds the whole value and
still takes it apart: `n @ (a, _)` names the pair `n` and its first component
`a`. `<(n.0, a) | add` is `1 + 1`.

A payload with several fields is one tuple. `Rect(w, h)` binds both. A record
payload can be opened in the same pattern, `At(Point { x, y })`. A variant
with no payload is written as a bare name, and a pattern must not give it
fields.

Constructors in a pattern are paths: `Hue::Red`, or `Red` after `cite
Hue::*;`. The type name stays a separate cite when you also write `Hue` in a
signature. `cite Hue::*;` brings the variants, and `cite Hue;` brings the
type.

## Binding in a header

A parameter is a pattern. The pattern must be irrefutable, because a header
has one arm.

```sl
func skew((a, b): (i64, i64), c: i64) -> i64 {
    <(a, c) | mul | x => (x, b) | sub
}

func norm(Point { x, y }: Point) -> i64 {
    <(x, x) | mul | z => (z, <(y, y) | mul) | add
}
```

## Leaving early

An arm that cuts to a continuation has type `(;)`. It does not decide the
type of the `of`, so another arm can return `(,)` and the statements after
the `of` still run on that path. The cut does not come back.

```sl
{{#include ../../examples/early.sl}}
```

```text
2
0
```

`first_even` walks a list. The even arm sends the number to `found` and
leaves. The odd arm returns `(,)`, and the caller, which captured `out` with
`mu`, then sends `0` to that same continuation. On `[1, 2, 3]` the cut
carries `2`. On `[1, 3]` the function returns and the caller sends `0`.

`mu i64 { out <= … }` captures the current continuation under the name `out`
and produces the `i64` that body sends to it. The [next
chapter](continuations.md) is that form used as the ordinary way to consume a
value.

The same early cut, flattened so the success path is not nested, is how
[`examples/programs/json_parser.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/json_parser.sl)
is written.
