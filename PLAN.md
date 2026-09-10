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

- **Captured continuations escape rather than resume.** The evaluator unwinds
  to the `mu` that captured a continuation, so one used after its `mu` has
  answered fails, saying so. Lifting this needs the evaluator to hold the
  context as data — an abstract machine with an explicit, re-instatable
  continuation stack — rather than as Rust stack frames.
  `examples/classical.sl` stays inside the limit deliberately.

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

Nothing is outstanding. New work goes here as it is planned.
