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

- **Effect rows are explicit and monomorphic.** A function declares its
  effects (`/ {E}`) and the checker enforces them, but there is no inference
  and no row polymorphism, so a higher-order function cannot forward an
  argument's effects — `map(f, xs)` cannot say it performs whatever `f` does.
- **Effect handlers are tail-resumptive only.** An operation's continuation
  is reified partly as `Prompt` frames and partly as the `Value::Kont`s the
  μ-based application lowering captures, which escape the handler delimiter.
  So a handler clause may use `resume` at most once and only in tail
  position — exceptions and tail-resumptive effects (readers, state-passing);
  multi-shot (nondeterminism) is rejected, not run. Lifting it needs
  application and `let` re-lowered to explicit frames so the whole
  continuation is the frame stack, at which point `resume` composes and
  repeats freely.

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

Nothing is outstanding. New work goes here as it is planned.
