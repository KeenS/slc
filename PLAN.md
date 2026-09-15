# Slant: the standing plan

Slant is a Rust-flavoured surface over a classical λ̄μμ̃ core. The redesign
that established it is finished and recorded in `docs/HISTORY.md`; the
language itself is defined in `DESIGN.md`. This file is neither — it holds
only what is still open: the known limits, the work queued next, and what is
deferred for discussion.

The contract that keeps it short: a decision, once made, goes to `DESIGN.md`
and leaves this file. The plan is where work stops being open, not where it
is remembered — so an entry here is a promise still outstanding, and nothing
else belongs. Entries refer to each other by name, never by position.

## Status

The acceptance suite covers the corrected runtime and effect rules as well
as complete design programs. Traits (ad-hoc
polymorphism) and algebraic effects with handlers are both shipped and
specified in `DESIGN.md`, and the execution model has been rebuilt around
them: the evaluator is an abstract machine over a closed, flat, de-Bruijn
instruction stream, its continuation first-class data (`DESIGN.md` §11). So
effect handlers are multi-shot, captured continuations are cheap and
reusable, and trait dispatch is resolved entirely at compile time.

The approved implementation queue is complete. Generic effects, composable
capture, and structural stage adapters are specified in `DESIGN.md`, with
runnable examples and regression coverage. The known design limits below remain.

Call-by-name is the settled direction for delayed computation: every demand
runs it afresh under the handlers around that demand. Future changes must keep
the polarity-directed evaluation discipline and explicit eager evaluation;
they do not replace it with a general call-by-value default. Call-by-need,
memoized thunks and implicit result caching are out of scope, not deferred
features. New features must preserve the separate forcing and activation
rows and the demand-time handler boundaries specified in `DESIGN.md`.

## Known limits

### Of the design

- **Structural adapters require a finite derivation.** Lifting through
  available declarations supports stored structures and regular recursion,
  but not recursive specialization with growing type arguments. Derivation
  is bounded, and opaque constructors still require matching arguments.
  There is no user-defined lifting interface for an opaque constructor, nor
  unrestricted commutative type equality. Capability rows are not mapped.

- **Stored handlers discharge concrete capabilities.** `Handler<A, B, E, F>`
  preserves both rows, but installation discharges only effects explicitly
  present in `E`; an unknown handled-row tail does not grant capabilities.
  Abstraction over unknown capability tails remains unsupported; typed
  generic applications inside concrete capabilities are supported.

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, and the untyped evaluator remains the backstop for whatever that
  gap hides. A type variable does carry the polarity of the generic parameters
  it meets (`DESIGN.md` §4, "Polarity by position").

## Next

Each entry starts with regressions, then implementation and documentation,
and passes `cargo fmt --check`, `cargo clippy --workspace --all-targets --
-D warnings` and `cargo test --workspace` before it is considered complete.
New or changed syntax also needs runnable examples with exact-output tests;
compiling complete `DESIGN.md` programs alone does not establish their
runtime behaviour.

No queued implementation tasks.

## Deferred, for discussion

Nothing is awaiting discussion.
