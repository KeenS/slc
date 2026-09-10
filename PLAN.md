# Slant Redesign Plan: λ̄μμ̃ and Additive/Multiplicative Duals

## Goal

Realign Slant with the classical lambda-bar-mu-mu-tilde calculus (λ̄μμ̃),
preserve a Rust-like surface, and make the syntax symmetric without pretending
that a Rust-like surface can itself be literally symmetric.

## Status

The redesign is finished, and the acceptance suite is green.

`DESIGN.md` is the language reference, and `docs/HISTORY.md` is the record of
what the redesign settled. This file holds only what is still open: known
limits, deliberate deferrals, and the work queued next. A decision, once
made, goes to `DESIGN.md`; this file is where it stops being open, not where
it is remembered.

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
  parameters. No receiver syntax: a method is called as a free function,
  `show(x)`, overloaded on the argument's type — Slant has no `.method()`
  and reads struct fields by `match`, so UFCS is the natural fit.

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
  form.
