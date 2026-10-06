# Patterns

Patterns appear in `of` and `mu` arms, in `let`, and in parameter lists. An
arm covers its scrutinee by patterns. A `let` or a parameter has a single
pattern, so that pattern is irrefutable: it matches every value of the type.
A refutable pattern belongs in an `of`.

| Pattern | Matches |
|---|---|
| `name` | Any value, and binds it |
| `_` | Any value |
| `name @ pattern` | The pattern, and binds the whole value as well |
| `literal` | That integer, float, string, or character |
| `-literal` | A negative numeric literal, the sign touching the digits |
| `start..=end` | An inclusive range; the endpoints are literals |
| `pattern \| pattern` | Either pattern |
| `(p, q)` | A tuple |
| `Name { field, field: pattern }` | A record. A bare field name binds a variable of that name |
| `Variant` | That variant, with no payload |
| `Variant(p, q)` | That variant's payload |
| `Enum::Variant(p)` | A qualified variant. The path may include a section |
| `::0(p)` | The alternative at that position of a sum |
| `.item(p)` | A request shape in a copattern |

A payload pattern may open a tuple or a record in place, because a product
has one shape. A sum nested in a payload is not split across arms; match it
with an `of` inside the arm.

Literal patterns and ranges are ordered by the value, and they are part of
an `of`. A `mu` arm does not use a literal pattern. Each `mu` arm covers one
shape, and a nested `of` distinguishes literals inside the arm.

Or-patterns and ranges share an arm, so they cover several values under one
body. Repeating a value across arms is an exhaustiveness error. Omitting a
variant is the same error. A wildcard covers the remainder, and a wildcard
that overlaps a value already covered is refused.

Qualified constructors are required when a bare variant name would be
ambiguous. Inside the standard library, enum variants used as patterns are
qualified for that reason.

The copatterns of `mu` are documented with the expression, under
[Expressions](expressions.md).
