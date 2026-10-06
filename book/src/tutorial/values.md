# Values

A value is data: a number, a string, a boolean, a tuple, a record. This
program uses each of them and prints one line per result.

```sl
{{#include ../../examples/values.sl}}
```

```text
5
Hello, SLC
false
30
3
A
1.5
S
SLC
true
```

## Literals

An integer literal takes the integer type its context requires. Sent to
`exit`, `0` is an `i32`. With nothing else to go on, it is an `i64`. A
floating literal takes `f32` or `f64` from its context and is an `f64`
otherwise. The widths are `i8`, `i32`, `i64`, `u8`, `u32`, and `u64`, and the
floats are `f32` and `f64`.

A minus sign that touches its digits is part of the literal: `-1`, `-1.5`.
Negating a computed value is the function `neg`, as in `<x | neg`.

A string literal is a `String`. A character literal is a `char`. The escapes
are `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, and `\'`.

`True` and `False` are the variants of `Bool`, which is an ordinary enum in
the prelude. Negation is `<b | not`. A `!`, `&&`, or `||` is a parse error
that names `not` or `of`. A choice on a `Bool` is an `of`:

```sl
of ready {
    True => <"yes" | println,
    False => <"no" | println,
}
```

`(,)` is the unit value, and an empty block is the same value. Its type is
written `(,)`. The name `unit` is that type as well.

## Arithmetic and comparison

Arithmetic and comparison are functions. A pair flows into the function:

```sl
<(2, 3) | add
<(10, 4) | sub
<(6, 7) | mul
<(20, 5) | div
<(7, 3) | rem
<(3, 5) | lt
```

The same pattern is `eq`, `ne`, `gt`, `le`, and `ge`. Writing `1 + 2` is a
parse error that names `add`:

```text
there is no `+` operator: flow the operands into `add`, `<(a, b) | add`
```

The pair is one argument. `<1 | add` does not wait for a second operand.

Division by zero and an overflowing arithmetic result are runtime errors.
Signed integers and the floats also have `neg`. Unsigned widths have no
`neg`.

`String` implements `Add`, so `<("Hello, ", "SLC") | add` concatenates.

## Let, tuples, and records

`let` binds a pattern. A name is the pattern that matches any value of its
type. A tuple pattern takes a tuple apart, and a record pattern takes a
record apart. The pattern has to match every value of the type, because
there is no second arm. A pattern that can fail belongs in an `of`.

```sl
let (a, b) = (10, 20);
let p = Point { x: 1, y: 2 };
```

`Point` is a `data` declaration. Constructing it names every field.
`.0` reads a tuple component, counting from zero. `.x` reads a record field.
`(10, (20, 30))` has two components, and its `.1` is the pair `(20, 30)`.

A tuple type is `(i64, i64)`. A longer tuple is `(A, B, C)`. The separators
are part of the type, as the [types reference](../reference/types.md)
explains.

## Strings

`index` returns the `char` at a zero-based position. `substring` returns the
characters from the start index up to, and not including, the end index.
`<(text, 7) | index` on `"Hello, SLC"` is `'S'`, and `<(text, 7, 10) |
substring` is `"SLC"`. An index or a slice outside the string is a runtime
error.

`str_len` is the number of characters. The [builtins](../reference/builtins.md)
page lists the rest of the string operations.

## Showing a value

`println` calls `Display`. Numbers print as digits, `Bool` prints `true` and
`false`, a string prints its characters, and `(,)` prints `(,)`. `to_string`
is the same rendering as a `String`, and `fmt` is the method underneath both.
