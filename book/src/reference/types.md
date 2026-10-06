# Types

A type denotes a value or a continuation. The sign and the connective say
which.

## Primitives

| Type | What it denotes |
|---|---|
| `i8`, `i32`, `i64` | Signed integers |
| `u8`, `u32`, `u64` | Unsigned integers, held in the same signed word, so a `u64` reaches as far as `i64` |
| `f32`, `f64` | Floating point |
| `char` | A Unicode scalar |
| `String` | A string of scalars |
| `Bool` | The prelude enum `False`, `True` |
| `File` | A file handle from `fs::open` |

Every integer is one machine word. A non-literal integer meets its port
exactly: a conversion is `Into`, described in the [prelude](prelude.md). An
integer literal may still take the width of the port it meets.

## Signs and parameters

`+T` is positive, `-T` is negative, and `*T` accepts either. On a generic
parameter the sign is written in the angle list: `<+T>`, `<-K>`, `<*T>`. An
unsigned parameter is a row variable.

`dual(T)` swaps the polarity. `dual` of a connective is the connective
opposite it.

## Arrows

| Spelling | Meaning |
|---|---|
| `A -> B` | A function from `A` to `B` |
| `B <- A` | A consumer transformer: give it a consumer of `B`, it consumes `A` |
| `A -> B / {E}` | The same function, performing `E` |
| `(-> T / {E})` | A by-name computation of `T`, performing `E` when forced |
| `(A hn B / {E} / {F})` | A handler value. Body `A`, answer `B`, discharged effects `E`, residual effects `F` |

`A -> B` and `B <- A` are different types. An empty effect row is omitted, so
`(-> T)` and `(-> T / {})` agree. An empty residual row on a handler is
omitted the same way.

`A -> (;)` is the consumer `-A`. `(;)` is the type of a command.

## Connectives

| Spelling | Unit | What a value of it holds |
|---|---|---|
| `(A, B)` | `(,)` | Both components, the product |
| `(A \| B)` | `(\|)` | One alternative, named `::0` or `::1` |
| `(A & B)` | `(&)` | A bundle of consumers or fields, one per position |
| `(A ; B)` | `(;)` | A joint: both sides, on the negative |

Longer forms associate to the right in the way a tuple does: `(A, B, C)`,
`(A | B | C)`. A joint `(k1 ; k2)` has no destructuring pattern. The name
`unit` is the product unit `(,)`.

A continuation row of a `proc` is a bundle, `(exit: A & other: B)`.

## Effect rows

```sl
/ {IO}
/ {Exn, Reader}
/ {..E}
/ {Exn, ..E}
```

A row is a set of effect applications. One effect name appears at most once.
`..E` is the rest of the row, and `E` is an unsigned generic parameter.
`Reader<i64>` and `Reader<String>` are different effects. Arguments of an
effect application are invariant.

A row on a negative type is latent: the effect happens when the value is
demanded, under the handlers around that demand.

## Polymorphism

```sl
func id<+T>(x: T) -> T
func map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
```

A `let` generalizes the type and row variables of a value. A computation,
including a `mu` that captured its continuation and a `do`, stays
monomorphic.
