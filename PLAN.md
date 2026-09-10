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

## Deferred, with no accepted replacement

- **Trait objects (`dyn`).** Dispatch is static — a concrete call goes direct,
  a bounded call through a dictionary — with no runtime method value, so there
  is no existential package that hides a value's type behind its trait. `dyn`
  (a value carried together with its dictionary) remains open.

- **The interaction-net backend.** An unwired experiment: `slc-core::net`
  and its bridge were reachable only from their own tests, never from the
  pipeline. Removed as dead code; git history has it, and the abstract
  machine the redesign built is the evaluator now.

## Next

- **Replace linearity with classical `⊥`-typing.** *(active)* The core is
  classical: continuations are the right-hand context of a sequent, which
  admits weakening and contraction. Requiring each declared continuation to be
  used (a *relevant* discipline) is therefore stricter than the semantics and
  rejects legitimate programs — a `command` that declares an error
  continuation it never triggers is well-formed classically, but the linearity
  pass rejects it. What soundness actually needs is only that a `command`
  reaches a continuation on **every path** — that its body has type `⊥`. Move
  that guarantee into the type checker and delete the linearity pass entirely,
  including the `⊗`-value single-use rule (which the classical core does not
  demand either).

  Stages:
  1. Type a no-`else` `if` as `unit` (its then-branch value is discarded), so
     a fall-through path is not `⊥`. Today it takes the then-branch's type,
     which lets `if c { x @ k }` masquerade as `⊥`.
  2. Require a `command` body to have type `⊥`; report an error otherwise
     (`main` included). This catches a body that falls off the end (`{ 42 }`)
     and a branch that dangles (`if c { x @ k }` with no `else`), while
     accepting a body that reaches *some* continuation and drops the rest.
  3. Delete `slc-check/src/linearity.rs`, its driver call, and its diagnostics
     wiring; drop the `linearity` module and diagnostic category.
  4. Repurpose `examples/linearity_error.sl` into a `⊥`-error example; update
     the two integration tests and the example expectation to the new message.
  5. Docs: retire "linear continuations" (goal 5 → classical control / the
     `⊥` rule), rewrite the continuation-rows paragraph (rows stay positional
     for the calling convention, but nothing is dropped-or-copied checked),
     drop the `linearity` diagnostics row and the `⊗`-value-linearity claims,
     and reframe the file-handle known limit (no value-linearity to build on).

- **Row-polymorphic effects.** Infer a function's effect row and let a
  higher-order function forward an argument's effects, so `map(f, xs)` can say
  it performs whatever `f` does — retiring the monomorphic-rows limit.
