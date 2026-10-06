# Expressions

## Blocks

A block is zero or more statements and a final expression:

```sl
{
    let x = 1;
    <(x, 2) | add
}
```

A semicolon follows every statement except the block's last expression. The
last expression is the block's value. An empty block is `(,)`. A block whose
last expression is a cut is a command.

A chain ends at the `;` or `}` of the block, and at the `,` or `)` that
closes a component. A component may itself be a chain that opens with `<`.

## let

```sl
let pattern = expression;
let+ pattern = expression;
let- pattern = expression;
```

The pattern is irrefutable. `let+` and `let-` are written with the sign
touching `let`. Polarity is [Polarity](polarity.md).

## of

```sl
of scrutinee {
    pattern => expression,
    pattern => expression,
}
```

An arm is a pattern, then `=>` for data or `<=` for a demand, then a body.
Arms cover the scrutinee exactly once. There is no guard between the pattern
and the arrow. A test is an `of` in the arm. The scrutinee's braces, when it
has them, are parsed as the `of` arms, so a record scrutinee is parenthesized
or bound with `let` first.

An arm that cuts has type `(;)` and does not fix the type of the `of`.

## mu

```sl
mu Type { pattern => command, … }
mu Type { copattern <= command, … }
mu Type { name <= command }
mu Type {}
```

`=>` builds a consumer of `Type`. `<=` with a copattern answers a menu or a
form. `<=` with one name captures the surrounding continuation and produces
`Type`. Empty braces are the consumer of a type that has no values.

A menu copattern follows the field. `item: out <= body` binds the
continuation. `item(param): out <= body` binds the arguments and then the
continuation. `item(param) <= body` binds the continuation under the field's
name.

A `mu` arm that matches data ends in a command: a cut to a consumer in
scope.

## Calls and constructors

A named function is applied by flow: `<argument | function`. Several
parameters are one group: `<(a, b) | function`. A nullary function is
`function()` or `<(,) | function`.

A constructor keeps its parentheses: `Cons(head, tail)`, `Point { x: 1, y:
2 }`. A variant with no payload is the name alone. An anonymous alternative
is `::0(value)`, counting from zero.

A menu demand is `menu.field` for the function, `menu.field(arguments)` for
the call, or `<arguments | menu.field`. A record or tuple projection is
`value.field` and `value.0`.

## do, hn, and reset

```sl
do expression handler
hn Effect { clauses }
hn [Effect, Effect] { clauses }
hn { clauses }
reset expression
```

`handler` is an `hn` expression, a name bound to one, or a `hand`. The body
of `do` is one expression, so a chain is parenthesized or written as a
block. Clause syntax is [Effects and handlers](effects.md).

`reset expression` installs a handler with no clauses. It bounds jumps and
discharges nothing.

## Chains

Chain forms are the whole of [Flow](flow.md). Inside a chain, `name =>
expression` names the value flowing in, and `name <= expression` names the
consumer the rest of the chain builds. A binder's body is one stage and ends
at the next `|`.
