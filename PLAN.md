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

The language is complete and the acceptance suite is green. Traits (ad-hoc
polymorphism) and algebraic effects with handlers are both shipped and
specified in `DESIGN.md`, and the execution model has been rebuilt around
them: the evaluator is an abstract machine over a closed, flat, de-Bruijn
instruction stream, its continuation first-class data (`DESIGN.md` §11). So
effect handlers are multi-shot, captured continuations are cheap and
reusable, and trait dispatch is resolved entirely at compile time. No large
feature is mid-flight; what remains open is below.

## Known limits

- **⅋ commutes only where one value meets one declared type.** `A ⅋ B`
  and `B ⅋ A` are one type, and a value is accepted at either spelling where
  it is cut into a consumer, stored in a record field, a variant or a
  `let`, passed as a written argument, or returned — the checker records a
  swap and lowering turns the closure around. Inside a type constructor —
  a tuple's component, `List<A ⅋ B>` against `List<B ⅋ A>` — there is no
  one value to turn, so the spelling still has to match; nor is an argument
  turned when it is the result of an earlier stage rather than a written
  value. Both would need the swap mapped through a structure or a chain.

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, type variables carry no polarity kind, and the untyped evaluator
  remains the backstop for whatever that gap hides.

- **Effect tracking follows names.** Rows and row variables are explicit
  and checked per declaration, but the rows live beside the type system
  rather than in core types: a lambda's effects are charged where it is
  written, a higher-order global passed as a value forwards nothing
  further, and a function laundered through a `let` binding is not
  tracked. Moving rows into the arrow type itself (unified during
  inference) is the known upgrade if these bite. A stage's row variables
  are instantiated only for the first stage of a chain, since that is the
  only one whose argument is syntax.

- **The file operations perform `IO` without an operation.** The `fs`
  module's `read`, `write`, `open`, `read_line`, `close`, and `exists`, and
  the `__` primitives beneath them,
  charge `{IO}`, so their rows are honest, but they reach the outside
  world directly rather than by performing an operation the way `println`
  does — so they cannot be mocked by a handler. Each offers its outcome to
  continuations, and an operation carrying an outcome needs a type the
  operation can name (a generic `IoOutcome<T>`, or one operation per
  outcome shape). Doing it needs a resumption point that dispatches on the
  outcome, which is a frame the machine does not have yet.

## Next

Nothing is queued.

## Deferred, for discussion

- **`(k1 ; k2)` as a pattern.** It was settled with the table, but a value
  of `;` is one consumer of both halves — it cannot be taken apart into the
  continuations it was built from, the way a bundle `(a & b)`, which holds
  both, can. Where such a pattern would bind anything, and what, is open.
- **Replacing `⟨` and `⟩`.** The cut brackets are the last non-ASCII
  surface syntax; their replacement is to be designed.
