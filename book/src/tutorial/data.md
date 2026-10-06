# Data

A cut sends a value to a continuation. The two share one square of shapes.

`data` and `form` carry every field. A `data` value gives the fields. A
`form` is a continuation that wants the fields.

`enum` and `menu` carry one alternative. An `enum` value is the variant it
picked. A `menu` value answers the one item the continuation picks.

The separators are the same square: `,` with `data`, `;` with `form`, `|`
with `enum`, and `&` with `menu`.

```sl
{{#include ../../examples/kinds.sl}}
```

```text
2
42
42
green
3
SLC
```

## Every field

`data Pair` is a value with both fields. `Pair { left: 2, right: 40 }` builds
one, and the program names it `p`. `p.left` reads the `left` field and prints
`2`. `of p` takes the whole value apart, and the arm names `left` and
`right`. Their sum is 42.

`form Total` is a continuation that receives both fields. `mu Total` builds
one. The arm names `left` and `right` together and sends their sum to `out`.
`Total { left: 2, right: 40 }` is the record that continuation receives.
`answer` is the continuation of the `i64`, and `<answer | total` supplies it.
The sum is 42 again.

Reading `t.left` is refused: `Total` is a form, and a form takes every field
at once: send it one, `<Total { … } | …>`.

A tuple `(2, 40)` is a `data` with the name left off, and its type is `(i64,
i64)`. A joint `(k1 ; k2)` is a `form` with the name left off.

## One alternative

`enum Colour` is a value that is one variant. `Colour::Green` builds the
green one. `of c` has an arm for each variant, each variant appears once, and
the order of the arms does not matter. `Green` produces `"green"`.

`menu Config` answers one item, and the continuation picks which. `mu Config`
builds the menu. The `retries` arm sends `3` to the continuation named
`retries`. The `name` arm sends `"SLC"` to the continuation named `name`.
`config().retries` picks the first and prints `3`. `config().name` picks the
second and prints `SLC`.

A choice `(i64 | String)` is an `enum` with the name left off: `::0(v)` and
`::1(v)` are the alternatives. A bundle `(k1 & k2)` is a `menu` with the name
left off.

### A field with arguments

A menu field may take arguments. The field is a function of those arguments.
The name after `:` is the continuation that receives the result. `item <=`
names that continuation after the field.

```sl
{{#include ../../examples/menu.sl}}
```

```text
5
```

`Pair` offers `both`. `numbers` builds a `Pair` whose `both` adds its
arguments and sends the sum to `out`. `numbers().both(2, 3)` calls that
function, and the sum is 5. The same call written as a stage is `<(2, 3) |
numbers().both`.

## Taking a scrutinee apart

`of` takes the named scrutinee in front of the braces apart. That scrutinee
is a value or a request, and the arm's arrow says which arrived.

A value arrives through `=>`. The arm is a pattern, then `=>`, then the
expression that pattern produces. A test on a bound name is another `of`
inside the arm.

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

A wildcard `_` covers whatever the earlier arms left. In `of 2 { 0 => "zero",
1 => "one", _ => "many" }` the wildcard is the remaining integers.

An inclusive range is `start..=end`. It takes the numeric type of the
scrutinee. `4` falls in `4..=6` and the arm produces `"mid"`.

A request is one menu item and the continuation that receives the answer.
`.retries(a)` asks for `retries`, and `a` receives the `i64`. A request
arrives through `<=`.

```sl
{{#include ../../examples/request.sl}}
```

```text
3
```

`reroute` has an arm for each item. `.retries(out)` binds the continuation
the request carries, and the arm answers with `.retries(out)` again.
`config()` is the menu from the first program. The cut `<config() |
(<.retries(a) | reroute)>` sends the request to the menu, and `a` receives
`3`.

The arrow follows what arrives, in an `of` and in a `mu`. A record arrives
in `mu Total`, so the arm is `Total { left, right } =>`. A request arrives
in `mu Config`, so the arm is `retries <=`. Every arm in one pair of braces
uses the same arrow.

### Or-patterns and `@`

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
type. [`early.sl`](https://github.com/KeenS/slc/blob/master/book/examples/early.sl)
cites `list::List` and `list::List::*` that way.

### Binding in a header

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

### Leaving early

An arm that cuts to a continuation has type `(;)`. Another arm can return
`(,)`, and the statements after the `of` still run on that path. The cut
does not come back.

```sl
{{#include ../../examples/early.sl}}
```

```text
2
0
```

`first_even` walks a list. The even arm sends the number to `found` and
leaves. The odd arm returns `(,)`. `mu i64 { out <= … }` names the
surrounding continuation `out`. When the function returns, the caller sends
`0` to that continuation. On `[1, 2, 3]` the cut carries `2`. On `[1, 3]`
the function returns and the caller sends `0`.

[Both arrows](continuations.md) writes a function whose result is a
continuation of its input. The same early cut, with the rest of the work
written after the `of`, is how
[`examples/programs/json_parser.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/json_parser.sl)
is written.
