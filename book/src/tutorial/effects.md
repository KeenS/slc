# Effects

An effect is a set of operations a computation may perform. A `hook` names
them. Performing an operation suspends the computation and asks the nearest
handler that answers that operation. `do` installs the handler around one
expression.

```sl
{{#include ../../examples/effects.sl}}
```

```text
-1
5
70
```

## Declaring and performing

```sl
hook Exn { func throw(message: String) -> i64; }

func checked_div(a: i64, b: i64) -> i64 / {Exn} {
    of (<(b, 0) | eq) {
        True => <"division by zero" | throw,
        False => <(a, b) | div,
    }
}
```

`throw` is an ordinary function in the scope of the hook. `checked_div`
names `{Exn}` on its arrow, so a caller can see the effect. A handler
discharges it:

```sl
let safe = do (<(10, 0) | checked_div) hn { throw(m) => -1 };
```

The clause binds the message and does not resume, so the handler's answer is
`-1` in place of the division. The clause for a successful division resumes
nowhere, because `throw` never ran, and the body's `5` is the answer.

`config` is a nullary operation. Its clause names the resumption:

```sl
let reading = do (<7 | scaled) hn {
    config(): resume => <10 | resume
};
```

`<10 | resume` supplies the operation's result. `scaled` multiplies `7` by
that `10`, and the handler answers `70`. A clause may resume once, more than
once, or not at all. The name `resume` is conventional; any name works in
that position.

The body's answer type is the handler's answer type when there is no
`return` clause. `return(x) => e` maps the body's value when the body
finishes through the normal result. Every operation clause produces that same
answer type, or leaves through a command.

## Forwarding

A handler that names any operation of an effect covers every operation of
that effect. A handler that means to leave some of them to an outer handler
ends with `_ => forward`. The forwarding clause is last, and it has no
binder.

```sl
{{#include ../../examples/forward.sl}}
```

```text
3
13
```

The first `do` answers both operations: `1 + 2` is `3`. The inner `do`
answers `first` with `10` and forwards `second`. The outer handler answers
`second` with `3`, so the sum is `13`. Forwarding keeps the effect in the
outward row. The outer `do` is what discharges it.

## Handlers as values

`hn Reader { … }` builds a handler and installs nothing. `do expr reader`
installs a handler that a `let` stored. The type of a stored handler is `(A
hn B / {E} / {F})`: the body produces `A`, the answer is `B`, the handler
discharges the concrete effects `E`, and the clauses may still perform `F`.
An empty residual row is left off the type.

```sl
let reader: (i64 hn String / {Reader}) = hn Reader {
    config(): resume => <10 | resume,
    return(value) => <value | to_string
};
```

`IO` is the effect the runtime handles, declared in the prelude as `write`
and `write_line`. `println` performs it. A program may install its own `IO`
handler nearer the operation. `main` may leave `{IO}` undischarged; anything
else is handled before the runtime sees it.

`fs::real`, `args::real`, and `clock::real` are hands. A `hand` is inlined
at each `do`, as in `do expr fs::real`. Using the hand's name as a value is
an error that tells you to install it with `do`.

Bare `reset expr` is a delimiter. It bounds a captured continuation and
discharges no effect. It is distinct from the library hand `control::reset`,
which answers `control::shift`.

The worked effects programs start at
[`examples/effects/effects.sl`](https://github.com/KeenS/slc/blob/master/examples/effects/effects.sl).
The reference page is [Effects and handlers](../reference/effects.md).
