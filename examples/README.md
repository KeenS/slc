# Examples of the updated language rules

## Generic effects and composable capture

[`generic_effects.sl`](generic_effects.sl) instantiates `Reader<T>` with an
integer and a string independently, then installs a stored
`Handler<i64, i64, {Reader<i64>}, {}>`. It prints `42`, `hello`, and `7` on
separate lines. Changing only the stored handler's effect argument to
`String` is rejected.

[`composable_capture.sl`](composable_capture.sl) passes a thunk to
`control::reset`. Its `control::shift` callback resumes with `1` and `2`;
the captured continuation multiplies each by ten, and the callback adds
the answers. It prints `30`. Both resumptions run the continuation afresh;
there is no caching. Bare `reset` does not handle this operation.

## Inferred call-by-name and nullary functions

[`inferred_demand.sl`](inferred_demand.sl) infers a callback's negative
polarity from its use. Discarding its computation does not perform `Build`;
demanding it twice performs `Build` twice under the demand handler. A
nullary function's name denotes the function, not a call: storing `answer`
does not print. `<(,) | factory` and `answer()` call it explicitly.

```text
0
demand
demand
3
stored
called
42
called
42
```

[`flow_evaluation.sl`](flow_evaluation.sl) compares flat pipelines, nested
applications and composition. Each discards an unused negative intermediate
without effects, and repeats a demanded one without caching.

## Yielding command exits and handler values

[`yielding_commands.sl`](yielding_commands.sl) supplies returning functions
for every command exit and continues with the selected result. It also shows
the equivalent explicit `mu` and an effectful callback. Its output is
`14`, `-14`, then `42`, on separate lines.

[`handler_values.sl`](handler_values.sl) stores handlers in a list, chooses
one through yielding exits, nests two installations, and maps a pure body's
answer through a stored `return` clause.

```text
140
72
42
```

## Delayed construction and explicit thunks

[`delayed_and_lazy.sl`](delayed_and_lazy.sl) stores
`Delayed<(i64 -> i64 / {Use}), {Build}>`, forces an alias under `Build`,
then invokes the resulting function twice under `Use` without rebuilding.
The original delayed value still rebuilds, including after conversion
through `lazy::of_delayed` and `lazy::to_delayed`. It also demonstrates
`Lazy` returning an effectful function without a positive wrapper.

```text
build now
ready
use
use
11
12
build again
23
lazy build
lazy ready
lazy use
34
```

[`lazy_effect_phases.sl`](lazy_effect_phases.sl) retains the explicit
factory and positive-wrapper encodings for comparison. Neither encoding
memoizes: the original thunk recomputes when demanded again.

Run these from the repository root, for example:

```sh
cargo run -p slc-driver -- run examples/handler_forwarding.sl
```

Each example below exits successfully. The example tests check its complete
output, exit status, and absence of diagnostics.

## Whole-effect handlers and forwarding

[`handler_forwarding.sl`](handler_forwarding.sl) contrasts a complete handler
with a partial handler ending in `_ => forward`. The partial handler supplies
`first`; `second` reaches the outer handler. Its function signature retains
`Config`, since forwarding does not discharge the effect. The forwarding
clause must be last.

```text
complete: 30
forwarded: 42
```

Without forwarding, a handler must name every operation of each effect it
handles, even if the current body uses only one operation.

## Escaping exits retain their effects

[`escaping_exits.sl`](escaping_exits.sl) passes a consumer to a generic command
that stores it in a record and returns the record through another exit.
The consumer's row follows it into `Saved<..E>`. Saving it performs nothing;
the handler around saving never receives `tick`. The handler around the later
cut does.

```text
stored without running
tick on activation
42
```

Changing the consumer's slot to pure `-i64`, or removing the activation
handler, is rejected. The command does not add `..E` to its own row because
it stores the consumer rather than activating it.

## Handler answer types and clause parameters

[`handler_answers.sl`](handler_answers.sl) handles an integer-producing
calculation with a `return` clause that produces a string. Calling `resume`
therefore returns that string, which the `seed` clause prefixes. The nullary
operation binds no parameters, while `combine` binds exactly two.

```text
resumed: answer: 42
```

All operation clauses must produce the handler's answer type or end in a
command. An operation's result type need not be the handler's answer type.

## Returning sinks versus consumers

[`consumer_returns.sl`](consumer_returns.sl) calls a sink of type
`i64 -> (,)`, then continues. It separately constructs a consumer of type
`-i64 / {IO}` and cuts into it. The returned consumer's row belongs on its
return type, not on its constructor's declaration. Construction does not
print; activation prints and exits.

```text
1
returned from sink
consumer constructed
2
```

Use an ordinary function and leave off the closing `>` for a returning sink.
A `select` arm must end in a command, not return unit.

## Stream rows and repeated demand

[`stream_effects.sl`](stream_effects.sl) returns a `Stream<i64, {Scale}>`
without performing `Scale` during construction. Repeated demands on the same
stream use different handlers, producing different results. The row also
survives conversion through `seq::of_stream` and consumption as a sequence.

```text
built, not demanded
[10, 20, 30]
[100, 200, 300]
[2, 4]
```

Demand is call-by-name: results are not memoized. Handling construction does
not handle later demands, and removing a demand's handler is rejected.

## Record fields and choice payloads

[`by_name_components.sl`](by_name_components.sl) stores a delayed function
computation in a record and a positional choice. These now follow the same
rules as tuple components and named variant payloads. The positive record
field computes immediately; storing the negative computations does not run
`Build`. Projection and matching retrieve them without forcing them.

```text
positive field
stored
record build
11
record build
22
choice build
33
```

The same record-field alias is demanded twice, under different handlers, so
construction repeats and uses each handler's answer. A pure callback slot
cannot hold this computation: its type must retain `Build`. To construct a
field eagerly, evaluate `make()` in a separate `let+` and store its result;
`let+ saved = Holder { ... }` does not recursively force negative fields.

## Projecting a delayed bundle

[`delayed_bundle.sl`](delayed_bundle.sl) keeps a bundle-producing computation
delayed. Each projection constructs the bundle under its current `Build`
handler. Retrieving the callback does not run the callback computation; its
separate `Use` effect happens only when the callback is applied.

```text
stored
build for value
10
build for callback
projected, not activated
build again
use callback
41
```

The `pending.0;` statement projects and discards the callback without
activating it. The final application demands both the bundle and its callback.
Repeated projection never caches the bundle, and handling its construction
does not remove the callback's `Use` requirement.
