# Polarity

Positive types denote values. Negative types denote continuations: functions,
consumers, and menus. The four places a type is written are all used.

| | Argument position | Continuation position |
|---|---|---|
| Positive type | Data arrives: `func f(x: i64)` | The input of a consumer transformer: `func f(k: i64) <- Request` |
| Negative type | A consumer arrives as data: `func f(note: -String)` | The exit of a `proc`: `proc f \| (k: -i64)` |

A `proc` splits its parameters by polarity, so a consumer parameter belongs
in the exit group. A returning `func` may take a negative argument, because
the function returns and the call does not promise to consume it.

A positive type in an exit position names what the continuation accepts.
`exit: i32` and `exit: -i32` are that spelling on `main`.

## When a computation runs

The position decides.

- A positive computation runs when the surrounding constructor, argument, or
  `let` runs.
- A negative computation stored in a binding, a tuple component, a record
  field, a variant payload, or a bundle item waits. It runs when it is
  demanded, and it runs again on every demand.
- Effects of a demand are handled by the handlers around that demand.

`let` follows the polarity it can see. An unfixed `let` is refused until it
is annotated.

| Binder | What it holds |
|---|---|
| `let` | The polarity of the expression |
| `let+` | Evaluates now. Shallow: a negative component of the result stays delayed |
| `let-` | A negative computation, rerun on every demand. A positive result is refused |

A by-name type is `(-> T / {E})`. Using the name forces it: `E` happens
then, and the result is `T`. The result's own effect row is separate.
`((,) -> T)` is a function applied to the unit value, and `(,)` is the unit
value.

The explicit thunk for either polarity is `lazy::Lazy<T, E>`, demanded with
`.force`. `lazy::of_delayed` and `lazy::to_delayed` convert negative
computations without running them. Neither form caches.

Unfixed forms are rejected: a lambda parameter with no polarity, a `mu` with
no type where one is required, an integer literal before a port that never
settles, a plain `let` whose polarity is unknown.

## Handlers and delay

`let+` forces the expression it binds. A command left under `let-`, or a
function that performs `Fs` only when it is later called, performs that
effect at the demand. The handler has to be around the demand. Handling the
name and forcing it later places the effect outside the handler.

[`examples/laziness/`](https://github.com/KeenS/slc/tree/master/examples/laziness)
is the suite for these boundaries.
