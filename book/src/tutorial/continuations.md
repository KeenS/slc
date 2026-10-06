# The other direction

`->` returns a value. `<-` produces a consumer. They are different types. A
value written one way is refused where the other is declared. A chain still
reads each stage in the orientation that stage was given, so both styles are
written left to right.

```sl
{{#include ../../examples/two-styles.sl}}
```

```text
75
42
```

## What `<-` declares

```sl
func area_of(out: i64) <- Shape {
    mu Shape {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul | out>,
        Rect(w, h) => <(w, h) | mul | out>,
    }
}
```

`out: i64` names the consumer of an `i64`: the positive type after the
parameter name is what that consumer accepts. `<- Shape` says the function
itself consumes a `Shape`. Give `area_of` somewhere to send an area, and it
will accept shapes.

`mu Shape` builds that consumer. Each arm is the same pattern an `of` would
use, and the arm ends in a cut. `<area | out>` sends the area to the consumer
the caller supplied. The rectangle arm does that directly. The circle arm
names the intermediate product with `x =>` and then cuts.

`<Shape::Rect(6, 7) | area_of | println` works because `area_of` is a stage
in the pipeline. The shape flows in, the area flows on, and `println` prints
it. The value-first call `<Shape::Circle(5) | area | println` is the same
pipeline with `area` in the middle.

## `of` and `mu`

| | data side | continuation side |
|---|---|---|
| declaration | `func f(x: A) -> B` | `func f(k: B) <- A` |
| taking apart | `of value { pattern => expr }` | `mu Type { pattern => command }` |
| how a result leaves | the arm's value | a cut `value \| consumer>` |

`=>` marks data flowing into an arm. `<=` marks a demand reaching back into
an arm. A `mu` that matches data uses `=>`. A `mu` that answers a menu, or
that captures the surrounding continuation, uses `<=`.

```sl
mu i64 { out <= <42 | out> }
```

The type in front of `<=` is what the expression produces. The type in front
of `=>` is what the consumer takes. Every arm in one pair of braces uses the
same arrow. Braces with no arms are the consumer of the written type, `mu
(|) {}`, and `(|)` is the type with no values.

## A menu

A menu answers one field when that field is demanded. A field with parameters
is the function of those parameters. `mu` answers the field with `<=`.

```sl
{{#include ../../examples/menu.sl}}
```

```text
5
```

`Pair` offers `both`. `numbers` builds a `Pair` whose `both` adds its two
arguments and cuts the sum onward. `numbers().both(2, 3)` demands that field,
and the sum is 5. The same demand written as a stage is `<(2, 3) |
numbers().both`.

A form wants every field supplied together.
[Declarations](../reference/declarations.md) lists it beside `menu`.

## A consumer literal

A lambda whose body is a cut is a consumer. Its type is `-String`, which is
the same type as `String -> (;)`.

```sl
fn(message: String) -> (;) {
    <message | println;
    <0 | exit>
}
```

The row of effects belongs on that consumer when activation performs them.
Constructing the lambda performs nothing. The cut performs the effects when
the consumer is used.

[`examples/duality/two_styles.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/two_styles.sl)
writes a whole program both ways, including the consumer that receives the
label and exits. [`examples/duality/polarity.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/polarity.sl)
is each of the four places a polarity can be written.
