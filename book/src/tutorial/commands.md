# Commands

A `proc` is a computation that ends in a cut. It takes a value group, then a
menu of exits. Each exit is a continuation, and a terminating path reaches
one of them.

```sl
{{#include ../../examples/commands.sl}}
```

```text
4
-3
```

## Exits

```sl
proc choose(n: i64) | (nonneg: (-i64) & neg: (-i64)) {
    of (<(n, 0) | lt) {
        True => <n | neg>,
        False => <n | nonneg>,
    }
}
```

The `|` in the header separates the values from the exits. `nonneg` and `neg`
are consumers of an `i64`. The body cuts the input to one of them. Writing
the positive type, `(nonneg: i64 & neg: i64)`, names what each exit accepts
and means the same row.

A call supplies the value, then one bundle of exits:

```sl
<4 | choose | (fn(n: i64) { n } & fn(n: i64) { 0 }) | println
```

The two functions return. Closing the exits on returning functions makes the
command yield the chosen result, and the chain continues into `println`. The
non-negative arm yields `4`. The negative arm of `-3` yields `-3`.

Closing the exits on `mu` consumers that themselves cut makes the call a cut,
and that cut ends the block. The program above uses returning functions so
`println` stays outside the command.

A row is positional. The bundle is written in the order the header declared,
the width matches, and the types match. An extra exit, a missing exit, or a
swapped pair is a different row. Writing `(nonneg: i64 & neg: i64)` names
what each exit accepts; `(-i64)` names the consumer itself.

## One outcome, one continuation

A result that can fail is a `proc` with one continuation per outcome. The
library's `list::nth` has `found` and `missing`. `fs::read` has `ok` and
`failed`. Exactly one of them runs.

```sl
<(xs, 2) | list::nth | (
    mu i64 { n => <n | println; <0 | exit> }
    & mu String { why => <why | println; <1 | exit> }
)>
```

The same shape is how `main` itself is typed: one exit, the status. A helper
ends the program only with a continuation it was given.

Anonymous sums carry the same choice as a value, for a program that keeps
the outcome and decides later. `::0(v)` and `::1(v)` are the alternatives,
counted from the left.

```sl
{{#include ../../examples/sum.sl}}
```

```text
3
hi
```

`(i64 | String)` is a sum with no declaration name. `of` matches `::0` and
`::1`. A longer sum is `(A | B | C)`, and the positions count along it.
