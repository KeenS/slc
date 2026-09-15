# Typed effects and composable capture

Status: implemented, including source generic effects, invariant argument
inference, scoped handler matching and the `control` library. `DESIGN.md`
defines the language rules. Existing non-generic effects keep their source
syntax and runtime behaviour.

## Effect identity in the core

A core row contains `Effect { name, args }` applications rather than bare
names. Its ordered set retains the whole argument vector: `Reader<+i64>`
and `Reader<+String>` cannot collapse into the same entry. An effect with no
arguments still prints as `IO`, not `IO<>`.

For example, this core type preserves both effect phases (its source
counterpart requires declarations for `Use` and `Build`):

```text
Delayed<(+i64 -> +String / {Use<+String>}), {Build<+i64>}>
```

It preserves the distinction between constructing the function under
`Build<+i64>` and applying it under `Use<+String>`. Type arguments may also
contain functions, row carriers, delayed types and further effect
applications. Core printing and parsing round-trip these nested types,
including explicit duals, connective units and row variables.

Type substitution, declaration instantiation, signature freshening and
scheme-variable collection traverse effect arguments. A parameter used in
an operation's result and effect row must be the same variable within one
signature instantiation, but different calls must freshen it independently.
The occurs check also traverses arguments: a variable cannot be bound to a
type whose row contains that variable recursively.

Row solving applies existing type substitutions before propagating complete
applications through flexible tails. At a named upper bound it infers
invariant argument equations. Nested row arguments generate bidirectional
row constraints, not erased annotations. Inference iterates until the
argument substitutions and constraints stabilize; failed equations roll back
without partially binding their arguments. This runs before pending trait
dispatch and polarity decisions are finalized.

## Source and handler boundaries

The source AST retains signed effect parameters and spanned applications in
written rows. Resolution preserves generic parameter scopes while qualifying
effect paths and named type arguments. It checks names, arity, kinds and
polarity, including unused declarations. Operation signatures retain the same
variables in their parameters, result and effect application.

A typed row set alone does not make runtime handler dispatch safe. Runtime
dispatch uses operation names, so a handler can intercept an operation even
if its type arguments differ from those of the handled application. The
checker rejects such an incompatible interception rather than treating
it as an unrelated effect that could escape through a residual row. Nested
handlers use scoped matching, not a global equation equating every use of
the same effect name. A handler's operation clauses share one application;
its body must agree with it even when forwarding leaves the effect outward.
Stored handlers preserve those applications in their capability rows.

## Composable capture

`control::Shift<+A, +R, E>` has one operation. Its parameter is
`Delayed<((A -> R / {..E}) -> R / {..E}), ..E>` and its result is `A`.
The thunk-taking `control::reset` handles this operation by passing its
resumption to the callback. It uses the existing runtime resumption value;
no new capture primitive or dispatch representation is needed.

Resumptions carry the residual row of their own handler body and clauses.
They do not inherit unrelated effects from the surrounding block, and do
not become pure merely because they are passed to a callback. Callback
construction is by-name; invoking it forces construction and then activates
the resulting function. The single budget `E` conservatively covers all
these phases, while `Delayed` keeps their demand boundaries distinct.

`A` and `R` are positive, and one installation has one fixed answer `R`.
Nested installations may use different types. This is not unrestricted
answer-type modification or a rank-polymorphic effect. The bare `reset`
delimiter still does not handle `Shift`; the library function is named
explicitly as `control::reset`. Neither introduces memoization or
call-by-need.

## Validation

`crates/slc-core/tests/effect_applications.rs` covers distinct applications,
propagation through tails, subtraction, substitution, instantiation, occurs
checks, latent declaration rows and core printing/parsing. Checker unit tests
cover signature freshening and scheme variables that occur only inside
effect arguments. `crates/slc-driver/tests/generic_effects.rs` covers typed
operations, stored and forwarding handlers, incompatible interception,
module scopes, nested capture, callback construction, multiple resumptions,
unrelated effects, answer mismatches and unhandled capture under bare
`reset`. Runnable examples `generic_effects.sl` and `composable_capture.sl`
have exact-output tests. Existing source-level tests protect non-generic
effects.
