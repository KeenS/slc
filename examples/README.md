# Examples

Every example runs with `cargo run -- run examples/<dir>/<name>.sl`, and the test
suite runs them all: `crates/slc-driver/tests/examples.rs` holds the output each
one must give.

## [`basics/`](basics) — Basic syntax

The everyday language: literals, functions, data, patterns, traits, modules. Start with `hello.sl`.

- [`arithmetic.sl`](basics/arithmetic.sl) — Arithmetic with surface operators
- [`array.sl`](basics/array.sl) — An immutable array: a 4-way trie of `Array4` nodes
- [`comparison.sl`](basics/comparison.sl) — Comparisons and boolean operators
- [`dictionaries.sl`](basics/dictionaries.sl) — Traits with no runtime method value: dispatch is resolved at compile time
- [`hash.sl`](basics/hash.sl) — `Hash`: a non-negative `u64` for a value
- [`hello.sl`](basics/hello.sl) — The simplest Slant program
- [`lambda.sl`](basics/lambda.sl) — Lambda abstraction and immediate application
- [`lists.sl`](basics/lists.sl) — Lists — an ordinary recursive enum, defined in the prelude
- [`maps.sl`](basics/maps.sl) — Ordered maps: a persistent AVL tree in the library
- [`match_exhaustive.sl`](basics/match_exhaustive.sl) — Match exhaustiveness checking
- [`namespaces.sl`](basics/namespaces.sl) — Modules: named scopes, flattened by resolution
- [`nested_calls.sl`](basics/nested_calls.sl) — Nested calls compose with surface operators
- [`pair.sl`](basics/pair.sl) — Tensor pairs construct with (a, b)
- [`patterns.sl`](basics/patterns.sl) — A binder is a pattern
- [`polymorphism.sl`](basics/polymorphism.sl) — Polymorphism: generic declarations, and `let` under the value restriction
- [`projection.sl`](basics/projection.sl) — Projection: `.i` reads a tuple component, `.field` reads a record field
- [`sets.sl`](basics/sets.sl) — HashMap, HashSet, and the ordered Set
- [`stdlib.sl`](basics/stdlib.sl) — The library has two layers, and this program draws on the second
- [`strings.sl`](basics/strings.sl) — String concatenation, indexing, and slicing
- [`sums.sl`](basics/sums.sl) — Anonymous sums
- [`traits.sl`](basics/traits.sl) — Traits: ad-hoc polymorphism by dispatch on a value's type

## [`duality/`](duality) — Both sides of the mirror

What is Slant's own: polarity, continuations as values, `mu` and `select`, menus and forms, and `|` as flow. Start with `two_styles.sl`.

- [`classical.sl`](duality/classical.sl) — Classical control: double negation elimination and excluded middle
- [`codata_impls.sl`](duality/codata_impls.sl) — Traits meet the negative side, in both directions
- [`command.sl`](duality/command.sl) — A `command` declaration with a value parameter and a continuation parameter, and the `mu` expression that captures one to pass it
- [`composition.sl`](duality/composition.sl) — The declaration square composes: a field can hold a type from any column, because a menu or form value is a value like any other
- [`connectives.sl`](duality/connectives.sl) — The connectives, in both polarities
- [`consumer_returns.sl`](duality/consumer_returns.sl) — A sink that returns, beside a consumer that does not
- [`data_functions.sl`](duality/data_functions.sl) — Functions over data types, in both polarities
- [`form.sl`](duality/form.sl) — `form` — the negative multiplicative, the mirror of `data`
- [`logical_units.sl`](duality/logical_units.sl) — The four logical units, each the nullary form of its connective: a paren holding only its separator — `(,)`, `(|)`, `(&)` and `(;)`
- [`menu.sl`](duality/menu.sl) — `menu` — the negative additive, the mirror of `enum`
- [`mu_escape.sl`](duality/mu_escape.sl) — mu captures the current continuation; activating it escapes with a value
- [`mu_tilde.sl`](duality/mu_tilde.sl) — μ̃ — the value abstraction
- [`pipeline.sl`](duality/pipeline.sl) — `|` is flow: everything moves left to right, and every step composes
- [`polarity.sl`](duality/polarity.sl) — Polarity by position
- [`select.sl`](duality/select.sl) — Negative additive construction
- [`structural_adapters.sl`](duality/structural_adapters.sl) — Functions stored where a negative type parameter asks for a consumer
- [`two_styles.sl`](duality/two_styles.sl) — The same program, twice: value-first, then continuation-first
- [`yielding_commands.sl`](duality/yielding_commands.sl) — A command's exits closed on returning functions, so the command yields a value

## [`effects/`](effects) — Effects and handlers

Algebraic effects, handlers as values, delimited control, and rows on the negative side. Start with `effects.sl`.

- [`composable_capture.sl`](effects/composable_capture.sl) — `control::shift` and `control::reset`: a captured continuation resumed twice
- [`delimited.sl`](effects/delimited.sl) — Delimited control: a handler delimits `mu`, and `reset` delimits without handling
- [`effects.sl`](effects/effects.sl) — Algebraic effects: a computation performs operations, a handler answers
- [`escaping_exits.sl`](effects/escaping_exits.sl) — An exit that escapes its command keeps the effects it performs
- [`generic_effects.sl`](effects/generic_effects.sl) — An effect with a type parameter, instantiated twice, and a stored handler
- [`handler_answers.sl`](effects/handler_answers.sl) — A handler's answer type, and the parameters of its clauses
- [`handler_forwarding.sl`](effects/handler_forwarding.sl) — A complete handler against one that forwards with `_ => forward`
- [`handler_values.sl`](effects/handler_values.sl) — Handlers as values: stored in a list, chosen, installed with `with … handle`
- [`io.sl`](effects/io.sl) — `IO`, the effect the runtime handles
- [`latent_effects.sl`](effects/latent_effects.sl) — The dual of effects: latent rows on the negative side
- [`multi.sl`](effects/multi.sl) — Several traits and several effects in one function

## [`laziness/`](laziness) — Evaluation on demand

Call-by-name components, `Delayed`, explicit laziness, and codata that is produced only as far as it is demanded.

- [`by_name_components.sl`](laziness/by_name_components.sl) — A delayed function stored in a record field and a choice payload, rebuilt at each demand
- [`delayed_and_lazy.sl`](laziness/delayed_and_lazy.sl) — `Delayed` construction against an explicit, caching `lazy` thunk
- [`delayed_bundle.sl`](laziness/delayed_bundle.sl) — A bundle-producing computation kept delayed, and projected without being run
- [`flow_evaluation.sl`](laziness/flow_evaluation.sl) — Flat pipelines, nested calls and `let`: when each stage is evaluated
- [`inferred_demand.sl`](laziness/inferred_demand.sl) — A callback's polarity inferred from its use; a nullary function's name is not a call
- [`lazy_effect_phases.sl`](laziness/lazy_effect_phases.sl) — Which handler is in scope when a delayed or lazy computation performs
- [`seq.sl`](laziness/seq.sl) — `Seq` — the finite codata sequence, and what it buys
- [`stream.sl`](laziness/stream.sl) — Streams — the prelude's coinductive mirror of `List` — and nested copatterns
- [`stream_effects.sl`](laziness/stream_effects.sl) — A stream whose demands perform an effect, handled where it is demanded

## [`programs/`](programs) — Programs

Practical code, written the way the language wants it written.

- [`file_io.sl`](programs/file_io.sl) — Input and output through continuations
- [`json_parser.sl`](programs/json_parser.sl) — A continuation-based JSON parser
- [`mealy_machine.sl`](programs/mealy_machine.sl) — A Mealy machine, as a `menu`: the input alphabet is the menu, and a state is a `mu`
- [`multi_file/`](programs/multi_file/main.sl) — A program in more than one file: `mod name;`, and the directory tree as the module tree
- [`regex_derivative.sl`](programs/regex_derivative.sl) — Regular expressions by Brzozowski derivatives, as a `menu`: a regex is what answers `nullable` and `derive`
- [`string_builder.sl`](programs/string_builder.sl) — A persistent string builder, expressed as a `menu` whose closed-over state is the accumulated text
- [`tree_search.sl`](programs/tree_search.sl) — Non-local jump: searching a tree

## [`errors/`](errors) — Programs that are refused

Each is wrong on purpose — two are ill-typed, one fails at run time — and the
test suite checks the diagnostic it gets.

- [`command_falls_through.sl`](errors/command_falls_through.sl) — This program is intentionally ill-typed: the command `bad` reaches no continuation
- [`delimited_error.sl`](errors/delimited_error.sl) — `reset` is a boundary: a jump from inside it to a continuation captured outside it is refused at run time
- [`polarity_error.sl`](errors/polarity_error.sl) — This program is intentionally ill-typed: it writes the one polarity combination that is rejected

# Notes on the updated language rules

What the examples written for the redesign show, with the output to expect.

### Generic effects and composable capture

[`generic_effects.sl`](effects/generic_effects.sl) instantiates `Reader<T>` with an
integer and a string independently, then installs a stored
`Handler<i64, i64, {Reader<i64>}, {}>`. It prints `42`, `hello`, and `7` on
separate lines. Changing only the stored handler's effect argument to
`String` is rejected.

[`composable_capture.sl`](effects/composable_capture.sl) passes a thunk to
`control::reset`. Its `control::shift` callback resumes with `1` and `2`;
the captured continuation multiplies each by ten, and the callback adds
the answers. It prints `30`. Both resumptions run the continuation afresh;
there is no caching. Bare `reset` does not handle this operation.

### Inferred call-by-name and nullary functions

[`inferred_demand.sl`](laziness/inferred_demand.sl) infers a callback's negative
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

[`flow_evaluation.sl`](laziness/flow_evaluation.sl) compares flat pipelines, nested
applications and composition. Each discards an unused negative intermediate
without effects, and repeats a demanded one without caching.

### Yielding command exits and handler values

[`yielding_commands.sl`](duality/yielding_commands.sl) supplies returning functions
for every command exit and continues with the selected result. It also shows
the equivalent explicit `mu` and an effectful callback. Its output is
`14`, `-14`, then `42`, on separate lines.

[`handler_values.sl`](effects/handler_values.sl) stores handlers in a list, chooses
one through yielding exits, nests two installations, and maps a pure body's
answer through a stored `return` clause.

```text
140
72
42
```

### Delayed construction and explicit thunks

[`delayed_and_lazy.sl`](laziness/delayed_and_lazy.sl) stores
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

[`lazy_effect_phases.sl`](laziness/lazy_effect_phases.sl) retains the explicit
factory and positive-wrapper encodings for comparison. Neither encoding
memoizes: the original thunk recomputes when demanded again.

Run these from the repository root, for example:

```sh
cargo run -p slc-driver -- run examples/effects/handler_forwarding.sl
```

Each example below exits successfully. The example tests check its complete
output, exit status, and absence of diagnostics.

### Whole-effect handlers and forwarding

[`handler_forwarding.sl`](effects/handler_forwarding.sl) contrasts a complete handler
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

### Escaping exits retain their effects

[`escaping_exits.sl`](effects/escaping_exits.sl) passes a consumer to a generic command
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

### Handler answer types and clause parameters

[`handler_answers.sl`](effects/handler_answers.sl) handles an integer-producing
calculation with a `return` clause that produces a string. Calling `resume`
therefore returns that string, which the `seed` clause prefixes. The nullary
operation binds no parameters, while `combine` binds exactly two.

```text
resumed: answer: 42
```

All operation clauses must produce the handler's answer type or end in a
command. An operation's result type need not be the handler's answer type.

### Returning sinks versus consumers

[`consumer_returns.sl`](duality/consumer_returns.sl) calls a sink of type
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

### Stream rows and repeated demand

[`stream_effects.sl`](laziness/stream_effects.sl) returns a `Stream<i64, {Scale}>`
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

### Record fields and choice payloads

[`by_name_components.sl`](laziness/by_name_components.sl) stores a delayed function
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

### Projecting a delayed bundle

[`delayed_bundle.sl`](laziness/delayed_bundle.sl) keeps a bundle-producing computation
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
