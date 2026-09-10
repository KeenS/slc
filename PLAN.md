# Slant: the standing plan

Slant is a Rust-flavoured surface over a classical λ̄μμ̃ core. The redesign
that established it is finished and recorded in `docs/HISTORY.md`; the
language itself is defined in `DESIGN.md`. This file is neither — it holds
only what is still open: the known limits, the deliberate deferrals, and the
work queued next.

The contract that keeps it short: a decision, once made, goes to `DESIGN.md`
and leaves this file. The plan is where work stops being open, not where it
is remembered — so an entry here is a promise still outstanding, and nothing
else belongs.

## Status

The language is complete and the acceptance suite is green. Two substantial
features are planned and specified below; both build on machinery already in
place, and they are ordered — traits first, effects second — because the
constraint handling traits introduce is the positive half of what effects
then mirror, and effects additionally reuse the resumable-continuation
machine the redesign already built.

## Known limits

- **A file handle's close is not enforced.** `+File` is the first resource
  with a lifetime, and the linearity checker does not yet watch it: an
  unclosed handle leaks until the program ends, and only a read after
  `close_file` fails. Watching it means value-linearity for one type —
  spent exactly once, by `close_file` or by being handed on. Until then the
  idiom is composition at the door: shadow `exit` with
  `select +i32 { status <= { close_file(handle); status @ exit } }` where the
  handle comes into scope, and no later path can leave the file open —
  `examples/file_io.sl` does exactly this.

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, type variables carry no polarity kind, and the untyped evaluator
  remains the backstop for whatever that gap hides.

## Deferred, with no accepted replacement

- **Negative partial application.** The partial-agent forms
  (`agent.consume(k, h)`, `fn.partial(a)`) are removed and nothing replaces
  them; a future design needs its own lowering and tests.
- **`choose T { Variant }`.** Removed along with its lowering, checking, and
  tests while its design is deferred.
- **The internal name `Command`.** The core types and evaluator still use it as
  a Rust type name. That is internal naming, not surface syntax, and is
  acceptable unless renamed separately.
- **`Result` and `Option` in the prelude.** Removed: error handling is
  continuation-based, so neither is canonical any more.
- **Static dictionary passing for traits.** Traits are implemented by
  dynamic dispatch on the argument's runtime type, checked total, rather than
  the static dictionary passing the plan first sketched: threading dictionaries
  through the unifier was too large a change to land safely, and runtime
  dispatch makes generic impls fall out for free. The zero-cost monomorphic
  version, and trait objects (`dyn`, the existential package), remain open.
- **The interaction-net backend.** An unwired experiment: `slc-core::net`
  and its bridge were reachable only from their own tests, never from the
  pipeline. Removed as dead code; git history has it, and an abstract
  machine (see the continuations limit) is the likelier evaluator future.

## Next

- **Algebraic effects and handlers: the negative dual of traits.** A trait
  hands a computation a dictionary of functions a value *provides*, resolved
  statically; an effect hands it a way to answer the requests a computation
  *demands*, installed dynamically by `handle`. Same dictionary shape, dual
  polarity — and effects need one ingredient traits do not: capturing the
  continuation at the operation and resuming it under the handler.

  *The runtime is the easy part — already built.* An operation captures the
  continuation up to its handler and the handler resumes it zero times
  (abort), once (normal), or many (nondeterminism) — exactly the resumable
  `Value::Kont` the abstract machine already reifies. The only addition is a
  **delimiter**: a `Prompt` frame on the machine's frame stack. `handle e
  with H` pushes one and runs `e`; `perform op(v)` scans the stack down to
  the nearest `Prompt` handling `op`, splits it there (`Vec::split_off`),
  packages the upper slice as a *composable* continuation `resume`, and jumps
  into the handler's clause with `(v, resume)`. `resume` is a `Kont` that
  *prepends* its slice rather than replacing the stack — the one
  generalization of today's whole-stack jump, and the stack-as-data substrate
  already represents both. This subsumes and makes ergonomic the
  `Request`-enum codata a provider answers by hand today (`connectives.sl`),
  adding the delimited resumption that pattern cannot express.

  *The real work is the type system.* A function's type carries an effect
  **row** — `fn foo() -> A / {State}` — pure functions having the empty row.
  `perform op` requires `op`'s effect in the ambient row; `handle` discharges
  one from it; higher-order functions forward rows by row polymorphism
  (`map<E>(f: A -> B / E, …) -> … / E`). Row unification and discharge are the
  substantial addition, dual to the trait checker's constraint set.

  *Linearity.* A handler's `resume` is unrestricted — it may be dropped
  (exception) or duplicated (nondeterminism) — unlike a `mu`-captured
  continuation, which must be consumed. That relaxation is the delicate
  soundness point and belongs in the effect's type: a handler declares
  whether it is one-shot or multi-shot.

  *Showcase.* Nondeterminism (`choose()`, resume twice, collect both) and
  exceptions (`throw()`, resume zero times, abort) — the two extremes that
  exhibit what the resumable machine uniquely enables, neither needing a
  parameterized handler.

  *Staging.* (1) `effect`/`handle`/`perform` syntax, AST, effect-row types;
  (2) the machine's `Prompt` frame, `perform` split, `handle` delimiter,
  composable `resume`; (3) effect rows in the checker — unification,
  discharge, row polymorphism; (4) `resume` linearity; (5) the nondeterminism
  and exception examples.

  *Deferred.* Parameterized (stateful) handlers, named/scoped handlers,
  effect-inference ergonomics, and tail-resumption as an optimization.
