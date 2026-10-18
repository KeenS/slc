# Continuations

A continuation is the rest of a computation. A cut sends a value into that
rest, and the rest runs. A `<-` function is given one or more of these rests
and produces a consumer. The consumer cuts to one of them.

This walk writes a product. Each kept element saves a continuation that
prepends `n*` to whatever the rest of the list produces. A zero cuts to a
different continuation, so those prepends never run, and the elements after
the zero are never visited.

```sl
{{#include ../../examples/continuations.sl}}
```

```text
resumed 1
visit 2
visit 3
resumed 2*3*1
visit 2
visit 0
stopped 0
```

## Two rests

`product` is given two continuations and produces a continuation of a list.
`resumed` accepts the product when every remaining element is kept.
`stopped` accepts the product when a zero is found. The type after each name
is what that continuation accepts. `<- List<i64>` is the consumer `product`
produces.

```sl
func product(resumed: String & stopped: String) <- List<i64> / {IO} {
```

`resumed: String & stopped: String` declares the two continuations, in that
order. A call passes the same bundle as one tuple.

The body is the consumer, written with `fn`. The list arrives as `xs`. The
body of that `fn` is a cut, so its result type is `(;)`.

`list::List` is `Nil`, or `Cons` of an element and the rest of the list.
[The library](library.md) defines it. `cite list::List` brings the type.
`cite list::List::*;` brings `Nil` and `Cons`. A constructor in a pattern is
a path, which [Data](data.md) covers. `of` chooses one arm, as it does for
`Bool` in [Values](values.md). The walk prints each element it reaches, so
`product` declares `{IO}`.

## The empty product

`Nil` has no factors. The arm cuts `"1"` to `resumed`.

```sl
Nil => <"1" | resumed>,
```

`<Nil | report` prints `resumed 1`.

## A zero leaves the saved rest unentered

A `Cons` prints the element, then matches it. Zero cuts `"0"` to `stopped`.

```sl
<("visit ", <n | int_to_str) | add | println;
```

```sl
0 => <"0" | stopped>,
```

On `Cons(2, Cons(0, Cons(9, Nil)))` the walk prints `visit 2`, then
`visit 0`, then `stopped 0`. The output has no `visit 9`: the cell after the
zero is never reached. The `2*` lives in the continuation that `2` passed
down. The zero cuts elsewhere, so that caller never prepends. Handing `"0"`
back would leave it in the caller's hands, and the line would be `2*0`.

## A kept element builds the next rest

Any other element calls `product` on the tail. It passes `stopped` through
unchanged. It passes a new `resumed`: `prepend` runs only when that tail
cuts a string to it. `int_to_str` writes the element as digits. `prepend`
joins those digits, `*`, and the tail's string, then cuts the result to the
`resumed` this step was given.

```sl
let prepend = fn(tail: String) -> (;) {
    let factor = <n | int_to_str;
    let suffix = <("*", tail) | add;
    let joined = <(factor, suffix) | add;
    <joined | resumed>
};
<rest | (<(prepend, stopped) | product)>
```

`<(prepend, stopped) | product` builds the consumer. `<rest | …>` gives it
the tail.

On `Cons(2, Cons(3, Nil))` the walk prints `visit 2`, then `visit 3`. The
empty tail cuts `"1"` to the `prepend` that `3` saved, which cuts `"3*1"` to
the `prepend` that `2` saved, which cuts `"2*3*1"` to the original
`resumed`. The line is `resumed 2*3*1`. Each `prepend` is the rest of the
product from that element on. Each one runs because the walk resumed.

## Both rests finish in one continuation

`report` prints whichever string the walk cuts. Its two exits both finish
by cutting a labelled string to `out`.

```sl
<mu String {
    out <= {
        let resumed = fn(text: String) -> (;) { <("resumed ", text) | add | out> };
        let stopped = fn(text: String) -> (;) { <("stopped ", text) | add | out> };
        <xs | (<(resumed, stopped) | product)>
    },
} | println
```

`mu String { out <= … }` names `out`: the continuation of this expression,
and the place both exits answer through. The string cut to `out` is the
value of the `mu` expression. The chain carries that string to `println` and
stops there, so the statement finishes and `main` runs the next list.
`println` performs `IO`, and so does the walk, so `report` declares that
row.

[Functions](functions.md) writes the other half of this cut. A function with
`->` returns to its caller, and that caller is the rest. `product` is given
its rest, and the cut decides which rest runs.

[Both arrows](arrows.md) writes `area_of` with `<-` and one continuation.
`mu` there takes a `Shape` apart. [Commands](commands.md) gives a `proc` a
menu of exits, one continuation for each way the command can finish.

The same saved rest, abandoned by a cut, is how
[`examples/programs/tree_search.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/tree_search.sl)
searches a tree, and how
[`examples/programs/json_parser.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/json_parser.sl)
leaves the rest of an input unread.
