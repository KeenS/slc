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

- **No partial application.** A stage supplies a callee's whole value
  group: `"high" | route` of a two-parameter `route` does not produce a
  function awaiting the second argument, so `7 | ("high" | route)⟩` is
  refused even though `route(tag, x) -> ⊥` would make the partial
  application a consumer of `+i64`. Calls are curried in the core and the
  runtime accumulates arguments, so this is a checker-side gap — the type
  of an under-applied call — not a representational one. (Found while
  establishing that operations need no negative form.)

- **A file handle's close is not enforced.** `+File` is the first resource
  with a lifetime, and nothing checks it: an unclosed handle leaks until the
  program ends, and only a read after `close_file` fails. Enforcing it would
  take a dedicated resource/ownership check (the value side of the language is
  otherwise unrestricted — see `DESIGN.md` §4). Until then the idiom is
  composition at the door: shadow `exit` with
  `select +i32 { status => { handle | close_file; status | exit⟩ } }` where the
  handle comes into scope, and no later path can leave the file open —
  `examples/file_io.sl` does exactly this.

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

- **The file builtins perform `IO` without an operation.** `read_file`,
  `write_file`, `open_file`, `read_line`, `close_file`, and `file_exists`
  charge `{IO}`, so their rows are honest, but they reach the outside
  world directly rather than by performing an operation the way `println`
  does — so they cannot be mocked by a handler. Each offers its outcome to
  continuations, and an operation carrying an outcome needs a type the
  operation can name (a generic `IoOutcome<T>`, or one operation per
  outcome shape). Doing it needs a resumption point that dispatches on the
  outcome, which is a frame the machine does not have yet.

## Next

Nothing is queued. The composition redesign's last box — irrefutable
patterns at binders — has shipped, and what remains below is limits and
deferrals, not work in flight.

## Deferred, for discussion

- **A surface spelling for `0`.** ⊤'s value is settled as `(&)` (above).
  `0` has no values, and its consumer stays `select Empty {}`; whether the
  empty sum deserves an anonymous type spelling is still open.

## Deferred, with no accepted replacement

- **Trait objects (`dyn`).** Dispatch is static — a concrete call goes direct,
  a bounded call through a dictionary — with no runtime method value, so there
  is no existential package that hides a value's type behind its trait. `dyn`
  (a value carried together with its dictionary) remains open.

- **The interaction-net backend.** An unwired experiment: `slc-core::net`
  and its bridge were reachable only from their own tests, never from the
  pipeline. Removed as dead code; git history has it, and the abstract
  machine the redesign built is the evaluator now.
