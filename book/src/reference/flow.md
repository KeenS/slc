# Flow

Everything in a chain moves left to right through `|`.

| Form | What it is |
|---|---|
| `<value \| function` | Application. The result is a value |
| `<value \| function \| consumer>` | A cut. The type is `(;)` |
| `function \| function` | Composition. The result is a function |
| `function \| consumer>` | Composition into a consumer |

`<` is required when a value starts the chain. A chain without `<` starts
with a function, so `"hi" | println` is refused. `<1` with no stage is
refused. `>` delivers to a consumer and does not return. A consumer stands
at the right end.

A stage is read in the orientation it was declared with. `area: Shape ->
i64` and `area_of: i64 <- Shape` are different types, and a chain does not
turn one into the other. Both still read left to right:

```sl
<shape | area | println
<shape | area_of | println
```

## The whole group

A stage receives the whole parameter group. `<1 | add` is a type error when
`add` takes a pair. `<(1, 2) | add` is the call. A named function is not
applied with parentheses. `f()` calls a nullary function, and the name `f`
alone is the function value.

Constructors keep parentheses: `Cons(1, Nil)`, `Point { x: 1, y: 2 }`.

## Commands in a chain

A `proc` takes the value group from the left and the exit bundle as the
closing stage:

```sl
<(xs, 2) | nth | (found & missing)>
```

Closing those exits on functions that return makes the command yield, and
the chain continues. Closing them on consumers that cut makes the whole
chain a cut.

## Binders

```sl
<1 | double | x => <(x, 1) | add
<12 | halve | ok <= (ok & odd) | halve | out>
```

`x => e` names the incoming value and is the function `fn(x) { e }`. `k <=
e` names the consumer the rest of the chain builds. The chain that uses `<=`
ends on a consumer. A binder's body is one stage.

## Where a chain ends

A chain ends at `;`, `}`, `,`, or `)` , whichever closes the expression that
holds it. A tuple written after `|` is one stage: the comma inside it does
not end the chain. A nested chain that is a component opens with its own `<`.
