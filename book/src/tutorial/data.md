# Data

[Values](values.md) and [continuations](continuations.md) meet in a cut.
Four declarations name the shapes of that meeting.

`data` and `form` carry every field. A `data` value gives the fields. A
`form` is a continuation that wants the fields.

`enum` and `menu` carry one alternative. An `enum` value is the variant it
picked. A `menu` value answers the one item the continuation picks.

Each declaration has an anonymous spelling. A tuple is `(A, B)`, a choice is
`(A | B)`, a bundle is `(A & B)`, and a joint is `(A ; B)`.

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

## `data`

A `data` declaration names a value that carries every field.

```sl
data Pair {
    left: i64,
    right: i64,
}
```

`Pair` is the name of the type. Each line inside the braces is one field: a
name, then the type of the value stored there. `left` stores an `i64`, and
so does `right`. The list order is the order later code writes.

### Building a value

`Pair { left: 2, right: 40 }` is a `Pair`. The literal names every field,
each expression has that field's type, and the names follow the declaration.
The program binds it to `p`.

Leaving `right` out is refused:

```text
`Pair` has fields `left`, `right`; the literal writes `left`
```

Writing `right` before `left` is refused:

```text
`Pair` has fields `left`, `right`; the literal writes `right`, `left`
```

### Reading one field

`p.left` is the `left` field, an `i64`. The program prints `2`. `p.right`
is the other field. A name that the declaration does not list is refused:

```text
`Pair` has no field `middle`
```

### Taking the value apart

`of p` takes the whole value apart. A `data` value has one shape, so there
is one arm, and the arm binds every field:

```sl
of p {
    Pair { left, right } => <(left, right) | add,
}
```

A bare field name binds a variable of that name. `Pair { left: x, right }`
binds the first field as `x` and the second as `right`. The fields in the
pattern follow the declaration. Reversing them is refused:

```text
`Pair` has fields `left`, `right`; the pattern writes `right`, `left`
```

The sum of `2` and `40` is `42`.

A parameter can open the value in the header. The pattern is that one
parameter, and it matches every value of the type:

```sl
func skew((a, b): (i64, i64), c: i64) -> i64 {
    <(a, c) | mul | x => (x, b) | sub
}

data Point { x: i64, y: i64 }

func norm(Point { x, y }: Point) -> i64 {
    <(x, x) | mul | z => (z, <(y, y) | mul) | add
}
```

[`examples/basics/patterns.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/patterns.sl)
runs these, and binds a tuple and a `Point` with `let` the same way.

### The tuple

A tuple `(2, 40)` is this connective with the name left off. Its type is
`(i64, i64)`. A longer tuple is `(A, B, C)`. Nesting matters: `(10, (20,
30))` has two components, and its `.1` is the pair `(20, 30)`. `.0` reads a
component, counting from zero. The unit `(,)` is the product with no fields,
and `(,)` is its value. The name `unit` is that type as well.

## `enum`

An `enum` declaration names a value that is one variant.

```sl
enum Colour {
    Red,
    Green,
}

enum Shape {
    Circle(i64),
    Rect(i64, i64),
}
```

`Red` and `Green` carry no payload. `Colour::Green` is a `Colour`.
`Circle` carries an `i64`, so `Shape::Circle` is a constructor, and
`Shape::Circle(5)` is a `Shape`. `Rect` carries two integers. Several
payload types are one tuple, so the constructor takes `(i64, i64)`, and the
pattern `Rect(w, h)` binds both components.

### One arm for each variant

`of c` has an arm for each variant. The arm's pattern is that variant. Each
variant appears once, and the order of the arms does not matter. `Green`
produces `"green"`, which the program prints.

Leaving a variant out is refused:

```text
non-exhaustive of: missing variant `Green` of enum `Colour`
```

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

A continuation of a `Shape` is `area_of` in [Both arrows](arrows.md). A
`Shape` value arrives in each arm there, so the arm arrow is `=>`, and each
arm sends the area on with a cut. An arm that is a bare value is refused.
`mu Colour { Red => 0, … }` reports:

```text
a `mu` arm is a command; this one has type +i64
```

### Literals, wildcards, and ranges

A wildcard `_` covers whatever the earlier arms left. In `of 2 { 0 =>
"zero", 1 => "one", _ => "many" }` the wildcard is the remaining integers.

An inclusive range is `start..=end`. It takes the numeric type of the
scrutinee. `4` falls in `4..=6` and the arm produces `"mid"`. A literal arm
and a range belong in an `of`. A `mu` arm covers one shape of a declaration,
and a nested `of` is where a literal is distinguished.

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

A payload that is itself a record can be opened in the same pattern,
`At(Point { x, y })`. A sum nested in a payload stays in that payload.
Match it with an `of` inside the arm.

Constructors in a pattern are paths: `Hue::Red`, or `Red` after `cite
Hue::*;`. The type name stays a separate cite when the signature also writes
`Hue`. `cite Hue::*;` brings the variants, and `cite Hue;` brings the type.
The [early cut](continuations.md) cites `list::List` and `list::List::*`
that way.

### The choice

A choice `(i64 | String)` is an `enum` with the name left off. `::0(v)` and
`::1(v)` are the alternatives, counting from zero, and `::0(x)` is the
pattern. A longer choice nests to the right, and `::2(v)` counts along that
nesting. The unit `(|)` is the choice with no alternatives, and it has no
value. An `of` on one has no arm.

[`examples/basics/sums.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/sums.sl)
builds choices with `::0` and `::1` and takes them apart.

## `menu`

A `menu` declaration names a value that answers one item. The continuation
that uses the menu picks the item.

```sl
menu Config {
    retries: i64,
    name: String,
}
```

`retries: i64` is an item whose answer is an `i64`. `name: String` is an
item whose answer is a `String`. A `Config` value has an answer for each
item. Using the value runs one of them.

### Building a menu

`config` returns a `Config`. `mu Config` builds it. Each arm is one item,
and each item appears once. A request arrives in the arm, so the arrow is
`<=`:

```sl
mu Config {
    retries <= <3 | retries>,
    name <= <"SLC" | name>,
}
```

`retries <= <3 | retries>` names the continuation carried by that request
`retries`, and sends `3` to it. The other spelling names the continuation
on its own:

```sl
retries: out <= <3 | out>
```

The type after `:` is what the continuation accepts. Here it accepts the
item's answer, an `i64`. The picked arm is the one that runs.

Leaving an item out is refused:

```text
non-exhaustive `mu Config`: missing items name
```

Writing the same item twice is refused:

```text
`mu Config` has duplicate arm for `retries`
```

### Picking an item

`config().retries` picks `retries`. The answer is the `i64` `3`, and the
program prints it. `config().name` picks `name` and prints `SLC`.

### A request

`.retries(a)` is the request itself: the item, and the continuation `a`
that receives the `i64`. Its type is `-Config`.

`of` takes a named request apart, one arm per item:

```sl
{{#include ../../examples/request.sl}}
```

```text
3
```

`reroute` has an arm for each item. `.retries(out)` binds the continuation
the request carries, and the arm answers with `.retries(out)` again. The cut
`<config() | (<.retries(a) | reroute)>` sends the request to the menu.
`reroute` answers with that same request, and `a` receives `3`.

### A field with arguments

A field may take arguments. The field is then a function of those arguments.
The type after `:` is the function's result, and in an arm the name after
`:` is still the continuation that receives that result.

```sl
{{#include ../../examples/menu.sl}}
```

```text
5
```

`both(x: i64, y: i64): i64` declares the item. A call supplies `(x, y)`, and
the result is an `i64`. The arm binds the arguments and then the
continuation:

```sl
both(x, y): out <= <(x, y) | add | out>
```

`x` and `y` are the arguments. `out` receives the sum. `numbers().both(2, 3)`
calls the function, and the sum is 5. The same call written as a stage is
`<(2, 3) | numbers().both`. The name `both` alone is the function.
`both(x, y) <= …` names that continuation `both`.

### The bundle

A bundle `(k1 & k2)` is a `menu` with the name left off. The continuation
that uses it picks one item by position. The unit `(&)` is the menu with no
items, and `(&)` is its value.
[`examples/basics/sums.sl`](https://github.com/KeenS/slc/blob/master/examples/basics/sums.sl)
sends a choice to a bundle of two continuations. The alternative's position
picks the continuation.

## `form`

A `form` declaration names a continuation that wants every field.

```sl
form Total {
    left: i64,
    right: i64,
}

func total(out: -i64) -> Total {
    mu Total {
        Total { left, right } => <(left, right) | add | out>,
    }
}
```

`left: i64` and `right: i64` name what flows in. `mu Total` builds the
continuation. A record arrives, so there is one arm, the arrow is `=>`, and
the arm binds `left` and `right` together. The arm is a cut: it sends their
sum to `out`.

`total` is a `->` function. Its result is the `Total`. The caller gives it
a continuation of the `i64` and gets the form back.

The record that feeds the form is `Total { left: 2, right: 40 }`. The field
names follow the declaration, as they do for `Pair`. Reversing them is
refused:

```text
`Total` has fields `left`, `right`; the literal writes `right`, `left`
```

In the program the cut is:

```sl
<mu i64 { answer <= <Total { left: 2, right: 40 } | (<answer | total)> } | println
```

Read it from the continuation outward. `answer` is the continuation of the
`i64`. `<answer | total` gives `total` that continuation, and the result is
a `Total`. The record is sent to that `Total`. The sum `42` reaches
`answer`, and `println` prints it.

A form takes every field at once. A parameter `t: Total` is the form, and
`t.left` is refused:

```text
`Total` is a form, and a form takes every field at once: send it one, `<Total { … } | …>`. A single field cannot be taken out of it — the others would have to be invented
```

### Several statements, and a second form

An arm with several statements uses braces. When those statements perform
`IO`, the form's declaration carries that row.

```sl
{{#include ../../examples/form.sl}}
```

```text
answer
42
relabelled!
7
```

`Report / {IO}` allows the arm to perform `IO`. `printer` prints `label`
and sends `value` to `out`. The first cut feeds `Report { value: 42, label:
"answer" }`. The label is printed, and `42` reaches `println`.

`shouting` is a `Report` built from another `Report`. Its arm rebuilds the
record with `"!"` added to the label and sends that record to `next`. In
`<a | printer | shouting`, `next` is the `Report` that `printer` returned,
so the label is printed with the extra `"!"` and the value `7` reaches
`println`.

[`examples/duality/form.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/form.sl)
is the same pair of forms.

### The joint

A joint is a `form` with the name left off. `(-i64 ; -i64)` is two
continuations of an `i64`, one for each component of a pair. `(k1 ; k2)`
builds that joint from the two continuations. [Continuations](continuations.md)
builds the same continuation in one arm, `mu (i64, i64) { (a, b) => … }`,
where the pair arrives together. A joint has no pattern that splits it back
into `k1` and `k2`. What is taken apart is the pair that feeds it.

```sl
{{#include ../../examples/joint.sl}}
```

```text
6
```

`(6, 7)` is sent to the joint. Delivery starts with `6` and the first
continuation. That continuation prints `6` and cuts to `exit` with `0`, and
the process status is `0`.

The unit `(;)` is the joint with no fields. It is the type of a cut, and it
has no value.

[Both arrows](arrows.md) writes the `<-` version of `total`: a function
given a continuation of the sum, returning a continuation of a `data` value
or a tuple.
