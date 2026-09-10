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
- **The interaction-net backend.** An unwired experiment: `slc-core::net`
  and its bridge were reachable only from their own tests, never from the
  pipeline. Removed as dead code; git history has it, and an abstract
  machine (see the continuations limit) is the likelier evaluator future.

## Next

- **Ad-hoc polymorphism: traits, by dictionary passing.** Rust-shaped
  `trait`/`impl` with bounds `<T: Show>`, elaborated the way modules are —
  traits exist only to a pass that rewrites them away, leaving the checker,
  lowering, and machine on constructs they already have.

  *The idea.* A dictionary is a struct — a positive product — of a trait's
  method values, and a bound `T: Show` is an implicit value parameter of that
  struct type. A monomorphic call supplies a concrete dictionary; a bounded
  generic forwards its parameter; a generic impl (`impl<T: Show> Show for
  List<T>`) is a dictionary-*building* function, resolved recursively. Since a
  dictionary lowers to a tensor and a method call to a field projection plus
  application — both already in the language — lowering and the runtime need
  no change.

  *Surface.* `trait`, `impl … for …`, `Self`, and bounds on declaration type
  parameters. A method is a free function, called `show(x)` and overloaded on
  the argument's type — **never** `x.show()`. This is settled, not a default:
  Slant has no receiver anywhere (it reads struct fields by `match`, not
  `x.field`), and a method is only a function whose meaning depends on an
  argument's type, so it should look like the function it is. `.` stays out
  of the value language entirely.

  *Where the work is — the checker.* Unlike module resolution, this cannot
  run before checking: the impl to pass is chosen from a type, so it is
  type-directed elaboration. Inference threads a constraint set; a bounded
  generic instantiated at a ground type discharges its constraint against a
  coherence-checked impl table (one impl per trait-and-head-type, trivial
  orphan rules while single-file); a constraint on a still-unknown variable
  propagates to the enclosing declaration's bounds or errors at a monomorphic
  site. Elaboration then inserts dictionary parameters, arguments, and
  projections. Soundness holds to the same informal standard: a dictionary is
  a value, so passing one runs nothing and the value restriction is
  untouched; coherence makes the checker's chosen impl the one that runs.

  *Continuations and linearity.* A method may be negative — a `command` or a
  `fn … <- …` that takes continuations — with no new machinery: a dictionary
  holds method *values*, and a negative function is a closure like any other,
  so a `Parse`- or `Emit`-style trait whose methods talk to continuations
  just works. The one real constraint is linearity. A bounded generic may
  call a method any number of times or none, so a dictionary must be
  duplicable: it is an **exponential**, `!Show<T>` — the linear-logic reading
  of a type class — and rides the value group as an unrestricted parameter
  (`Type::Bang`, and the checker's `is_unrestricted`, both already exist). A
  method's *continuation arguments* stay linear per call; the dictionary that
  supplies the method does not. And because impls are resolved globally and
  coherently, the dictionary a captured continuation closes over is fixed:
  reinstating that continuation re-uses the same impl, so there is none of the
  dynamic-scope hazard that implicit or dynamically-scoped instances would
  carry across a jump. Bounds are on positive type variables only in v1;
  bounding a negative variable — a trait describing what a *provider* of
  codata must offer — is the dual and is deferred with the rest.

  *Staging.* (1) syntax + AST; (2) impl table + coherence; (3) constraints in
  inference with ground resolution; (4) elaboration to dictionary structs,
  params, and projections; (5) generic impls, resolved recursively; (6)
  method-name overload resolution; (7) a `Show`-and-`List` example plus
  must-reject tests for a missing impl and overlapping impls.

  *Deliberately deferred.* Associated types, default methods, supertraits
  (beyond a trivial `trait Ord: Eq`), and — notably — trait objects: `dyn
  Trait` is the existential package `∃T. (↓T ⊗ Show<T>)`, so it waits on the
  `∀`/`∃` quantifiers already sketched in the shifts discussion. Traits give
  the constraint machinery; the quantifiers give it a first-class dynamic
  form. Further out, the dual of a trait is an **effect**: a dictionary of
  functions a value *provides* becomes a dictionary of continuations a
  computation *demands* — algebraic effects and handlers as the negative
  mirror of type classes, on the same dictionary machinery.

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
