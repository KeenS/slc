# The SLC Programming Language

SLC is a programming language. A program is a command: it sends a value to a
continuation, and that meeting is a step of the program. Its core follows the
classical λ̄μμ̃ calculus (lambda-bar-mu-mu-tilde). Source files end in `.sl`.
The command-line tool is `slc`.

There is one grammar. A type denotes a value or a continuation, and the arrow
says which way a function faces, so the same program can be written
value-first or continuation-first. The early chapters write functions that
return values. [Data](tutorial/data.md) gives values and continuations the
same shapes: `data` and `form`, `enum` and `menu`.

This book is the tutorial and the reference for the language implemented in
the [SLC repository](https://github.com/KeenS/slc). The specification is
[`DESIGN.md`](https://github.com/KeenS/slc/blob/master/DESIGN.md), with the
parts under
[`docs/design/`](https://github.com/KeenS/slc/tree/master/docs/design). Where
this book and the specification disagree, the specification and the compiler
decide.

The programs printed in the tutorial live in
[`book/examples/`](https://github.com/KeenS/slc/tree/master/book/examples).
`book/check.sh` runs them and compares the output.

## Two ways through one pipeline

A function written with `->` is given a value and returns a value.

```sl
func area(s: Shape) -> i64 {
    of s {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul,
        Rect(w, h) => <(w, h) | mul,
    }
}
```

`of s` takes the shape apart. With 3 standing in for π, the circle of radius
5 has area 75.

A function written with `<-` is given a continuation of the result and
returns a continuation of the input.

```sl
func area_of(out: i64) <- Shape {
    mu Shape {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul | out>,
        Rect(w, h) => <(w, h) | mul | out>,
    }
}
```

`out: i64` is the continuation of the area. `mu Shape` builds the
continuation of the shape, and each arm sends the area to `out`.

`<` opens a chain with a value, and `|` carries that value from left to
right. A chain reads each stage in the orientation that stage was written
with, so both functions sit in the same pipeline. [Both
arrows](tutorial/continuations.md) runs this program.
The whole dual example in the repository is
[`examples/duality/two_styles.sl`](https://github.com/KeenS/slc/blob/master/examples/duality/two_styles.sl).
