# Effects and handlers

## hook

```sl
hook Exn { func throw(message: String) -> i64; }
hook Reader<+T> { func read() -> T; }
hook Shift<+A, +R, E> {
    func shift(callback: (-> ((A -> R / {..E}) -> R / {..E}) / {..E})) -> A;
}
```

An operation is a function. Its result type is what a resumption receives. A
generic hook's operations share its parameters. A written application
supplies every argument. Parameters are not repeated and do not carry trait
bounds. A row holds at most one application of a given effect name.

A function that performs the effect names it:

```sl
func checked_div(a: i64, b: i64) -> i64 / {Exn}
func map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
```

A call instantiates row variables from the arguments that mention them.
Inside the body the variable is rigid. A rowless arrow is a promise of
purity: passing a function that performs `Exn` where `(i64 -> i64)` is
declared is an error. A pure function may still be passed where extra
effects are allowed.

One `do` uses one instantiation for every operation of an effect.
`{Reader<i64>}` and `{Reader<String>}` are different capabilities.

## do and clauses

```sl
do body hn {
    throw(message) => -1,
    config(): resume => <10 | resume,
    return(value) => <value | to_string,
    _ => forward,
}
```

An operation clause binds as many parameters as the operation declares. A
nullary operation is written `config()`, with no unit binder. The
continuation is bound after a colon. Omitting it means the clause never
resumes.

`return(x) => e` maps the body's value. Without it, the handler's answer
type is the body's type. Every operation clause produces that answer type or
ends in a command. The resumption returns the handler's answer, including a
`return` clause's transformation, and it keeps the residual row of that
handler.

A handler covers a whole effect. Naming any operation requires the others. `_
=> forward` is the last clause, has no binder, and passes unmatched
operations outward. The forwarded effect stays in the outward row.

`do` installs a handler around one expression. A `|` after the whole `do`
belongs to the surrounding chain.

## hn and hand

```sl
hn Reader { clauses }
hn [Reader, Other] { clauses }
hn { clauses }
let reader: (i64 hn String / {Reader}) = hn Reader { … };
do body reader
```

`hn` builds a value and runs nothing. The type is `(A hn B / {E} / {F})`:

| Parameter | Meaning |
|---|---|
| `A` | What the body produces |
| `B` | The handler's answer. Equal to `A` when there is no `return` clause |
| `E` | Concrete effects the clauses discharge |
| `F` | Effects the clauses and the residual body still perform |

All four arguments are invariant. An annotation cannot grant effects the
clauses do not answer. An open tail on `E` grants no unknown capability.
Installation discharges only the effects written in `E`. Construction does
not generalize the handler out from under the instantiation its clauses
fixed.

A `hand` is inlined at each `do` and is not a value. Its answer type is the
body's, including `(;)`. The row written on the `hand` is what the clauses
perform. The diagnostic for using a hand as a value says to install it with
`do expr name`.

## Rows and polymorphism

A handler discharges concrete effects named in the type. An effect that
arrives only through a row variable, inside a closure the caller passed,
passes through to the caller's handler.

A `let` generalizes a stored `hn` only under the value restriction, and only
after the clause arguments have been equated with the capability row. A `do`
expression is not a value, so its result is not generalized.

Effects on a `menu` or a `form` are latent. They run when the field is
demanded.

## reset

`reset expression` is a delimiter: a handler with no clauses. It bounds a
continuation captured inside it and discharges nothing. A jump from inside
it to a continuation captured outside it is a runtime error.

`control::reset` is a library `hand` of `control::Shift`. It is installed
with `do expr control::reset`. Bare `reset` does not handle `Shift`.

## IO

```sl
hook IO {
    func write(text: String) -> (,);
    func write_line(text: String) -> (,);
}
```

The runtime handles `IO` around `main`. `main` may leave `{IO}` in its row.
A nearer handler answers first.
