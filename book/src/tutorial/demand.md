# Demand

A type is positive or negative on its own. Positive types are data: numbers,
strings, tuples, records, enums. Negative types are functions, consumers, and
menus. Where the computation sits decides when it runs.

- A positive computation in a `let`, an argument, or a constructor runs when
  that constructor runs.
- A negative computation stored in a binding, a field, a tuple component, or
  a variant payload waits until something demands it, and runs again on every
  demand.
- The handlers around the demand are the handlers that see its effects.
  Handling the binding earlier leaves the later demand untouched.

Call-by-need, memoized thunks, and an implicit cache are outside the
language. A delayed computation runs afresh.

```sl
{{#include ../../examples/demand.sl}}
```

```text
stored
10
20
3
```

## `let`, `let+`, and `let-`

Plain `let` follows the polarity of the expression. When the polarity is
still unknown, the binding is refused until it is written `let+` or `let-`.

`let-` holds a negative computation and reruns it on each demand. A positive
result under `let-` is refused. In the program, `fn { make() }` is a
function, so `let-` accepts it. The line `stored` prints before either call.
Each `<(,) | pending` runs `make` again, under the handler of that `do`. The
first demand answers `10`. The second answers `20`. Nothing is cached.

`let+` evaluates now. `let+ eager = do make() hn { … }` runs `make` once,
under that handler, and binds the integer `3`. `let+` is shallow. It does
not walk into a negative value stored inside the result and force that too.

A by-name type is written `(-> T / {E})`. The blank domain is the unit of
the arrow. Forcing it performs `E` and produces `T`. The result may itself
be a function with a further effect row: `(-> (i64 -> i64 / {Use}) /
{Build})` performs `Build` when forced and `Use` when the function is
applied.

## An explicit thunk

A delayed positive result has a menu in the library, `lazy::Lazy<T, E>`,
demanded with `.force`. `lazy::of_delayed` and `lazy::to_delayed` convert a
negative computation to and from `(-> T / {E})` without running it, and
without caching it.

```sl
menu Lazy<*T, E> / {..E} {
    force: T,
}
```

The `*` says the parameter accepts either polarity. The menu's row is the
row the force performs.

[`examples/laziness/delayed_and_lazy.sl`](https://github.com/KeenS/slc/blob/master/examples/laziness/delayed_and_lazy.sl)
separates the `Build` phase from the `Use` phase.
[`examples/laziness/stream.sl`](https://github.com/KeenS/slc/blob/master/examples/laziness/stream.sl)
is the coinductive stream, produced only as far as a demand reads it.
