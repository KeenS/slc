# Functions

A `func` takes data and returns data. The parameter list is one group, in
the order written, and a call supplies that whole group.

```sl
{{#include ../../examples/functions.sl}}
```

```text
42
Hello, SLC
36
7
5
```

## Declaration and call

```sl
func double(n: i64) -> i64 {
    <(n, 2) | mul
}
```

`-> i64` is the result. `<21 | double` sends one integer in. A function of
several parameters takes them as one tuple, in the declared order:

```sl
func greet(name: String) -> String {
    <("Hello, ", name) | add
}
```

`<"SLC" | greet` is the call. `<("Hello, ", "SLC") | greet` would be a type
error, because `greet` takes one `String`. Partial application is a type
error as well: the group arrives whole, or the call is refused.

The body is a block. Its value is the last expression. A semicolon separates
statements before that expression.

A parenthesized call `double(21)` is refused for a named function. The
diagnostic tells you to flow the argument in. Two spellings do use
parentheses: a nullary call, `f()` or `<(,) | f`, and a constructor,
`Shape::Circle(5)`. The name `f` alone is the function, and naming it runs
nothing.

## Lambdas

`fn` is a function with no declaration name.

```sl
let square = fn(n: i64) -> i64 { <(n, n) | mul };
<6 | square | println;
```

A lambda parameter needs a polarity the checker can see. A type annotation
supplies it. Immediate application of a lambda keeps the parentheses the
declaration does not: `<fn(x: i64) -> i64 { x }(42) | println`.

## Generics

A type parameter is declared in angles, with a sign. `+` means the parameter
stands for a value type.

```sl
func id<+T>(x: T) -> T { x }
```

Each call chooses its own `T`. `<7 | id` is an integer, and `<"seven" | id`
is a string. Inside the body, `T` is rigid: the function cannot inspect it
except through a trait bound, which the [traits](traits.md) chapter adds.

A `let` generalizes a value, such as a lambda or a variant that carries no
payload. A computation stays monomorphic. The usual way to name a polymorphic
function is a `func` declaration.

## Effects on the arrow

A function that performs an effect says so after the result:

```sl
func greet(name: String) -> (,) / {IO} {
    <name | println
}
```

A bare `->` is an empty effect row: the function performs nothing. The
[effects](effects.md) chapter is where a row is handled.
