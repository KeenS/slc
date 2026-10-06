# Both arrows

`->` and `<-` are the two directions a function can face. They are different
types. A chain reads each stage in the direction that stage was written, so
both sit in the same pipeline, left to right.

```sl
{{#include ../../examples/two-styles.sl}}
```

```text
75
42
```

## `->` and `<-`

```sl
func area(s: Shape) -> i64 {
    of s {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul,
        Rect(w, h) => <(w, h) | mul,
    }
}

func area_of(out: i64) <- Shape {
    mu Shape {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul | out>,
        Rect(w, h) => <(w, h) | mul | out>,
    }
}
```

`area` is given a `Shape` and returns an `i64`. `of s` takes that value
apart, and the arm's value is the area.

`area_of` is given a continuation of an `i64` and returns a continuation of
a `Shape`. `out: i64` names the continuation it is given: the positive type
is what that continuation accepts. `<- Shape` is the continuation this
function returns. `mu Shape` builds it, one arm for each variant. A `Shape`
value arrives in the arm, so the arrow is `=>`, the same arrow `mu Total`
uses when [Data](data.md) builds a form. Each arm sends the area to `out`.

`<Shape::Circle(5) | area | println` sends the circle through `area`.
`<Shape::Rect(6, 7) | area_of | println` sends the rectangle through
`area_of`. In both chains the area reaches `println`.

The arrow on the function is the direction it faces. A function written with
`->` returns a value, and that value may be a form or a menu.
[Data](data.md) builds both with `mu`: `total` returns a `Total`, and
`config` returns a `Config`. A function written with `->` may also be given
a request and take it apart: `reroute` is given a `-Config`.
[Continuations](continuations.md) writes `sum_into` in the same shape as
`area_of`, for a tuple instead of an enum.

[`examples/duality/two_styles.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/two_styles.sl)
writes a whole program both ways.
[`examples/duality/polarity.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/polarity.sl)
is each of the four places a polarity can be written.
[`examples/duality/connectives.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/connectives.sl)
is `data`, `enum`, `menu`, and `form` in one program.
