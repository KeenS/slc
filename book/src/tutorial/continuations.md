# Continuations

A continuation is what a value is sent to. `exit` in
[the first program](first-program.md) is one: the runtime supplies it, and
the integer sent to it is the process status. This program builds
continuations and sends values to them.

```sl
{{#include ../../examples/continuations.sl}}
```

```text
5
Hello, SLC
SLC
13
shown
```

## Naming one

`mu i64 { out <= … }` names the continuation around that expression `out`.
The body sends an `i64` to `out`. The value `out` receives is the result of
the `mu`.

```sl
<mu i64 { out <= <(2, 3) | add | out> } | println
```

`<(2, 3) | add` is `5`. The cut `<5 | out>` delivers it. `println` receives
that `5`. `mu String { … }` is the same shape for a string, and the second
line prints `Hello, SLC`.

The arm arrow is `<=`. A request for this surrounding continuation arrives
in the arm, and `out` is the continuation the request carries.

## A continuation parameter

`out: -String` is a continuation of a `String`. The type under the minus is
what the continuation accepts. `emit` sends its text there:

```sl
func emit(text: String, out: -String) -> (;) {
    <text | out>
}
```

The body is a cut, so it has type `(;)`. It does not come back to `emit`.
The caller names where the text goes:

```sl
<mu String { out <= <("SLC", out) | emit> } | println
```

`out` is the continuation of that `mu String`. `emit` sends `"SLC"` to it,
the `mu` results in `"SLC"`, and `println` prints it.

`-String` and `String -> (;)` are the same type. The positive spelling
`out: String` on a `<-` function is the same continuation again: the type
written after the name is what it accepts. `sum_into` is written that way.

## A continuation of several values

A tuple arrives whole, so a continuation of a tuple has one arm and binds
every component:

```sl
func sum_into(out: i64) <- (i64, i64) {
    mu (i64, i64) {
        (a, b) => <(a, b) | add | out>,
    }
}
```

`out: i64` is the continuation of the sum. `<- (i64, i64)` is the
continuation this function returns, a continuation of the pair. The arm
arrow is `=>` because the pair arrives as a value. The arm sends the sum
with a cut. `<(6, 7) | sum_into` sends the pair in, and the sum `13` reaches
`println`.

[Data](data.md) names these shapes. A `form` is a continuation that wants
every field, the way `sum_into` wants both components. A `menu` answers one
item the continuation picks.

## Writing one with `fn`

A function whose body is a cut is a continuation. `-String` is its type.

```sl
fn(message: String) -> (;) {
    <message | println;
    <0 | exit>
}
```

`println` runs, then the cut sends `0` to `exit`. The call closes with `>`,
because the function is the continuation the string is delivered to:

```sl
<"shown" | fn(message: String) -> (;) {
    <message | println;
    <0 | exit>
}>
```

That line prints `shown` and ends the program. `println` and `exit` perform
`IO` when the string is delivered. `main` declares that row.

## Leaving early

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
leaves. The odd arm returns `(,)`. The caller names the surrounding
continuation `out`. On `[1, 2, 3]` the cut carries `2`, so the `0` after
the call is not sent. On `[1, 3]` the function returns and the caller sends
`0`.

The same early cut, with the rest of the work written after the `of`, is how
[`examples/programs/json_parser.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/json_parser.sl)
is written.

[Both arrows](arrows.md) writes one function with `->` and the same function
with `<-`.
