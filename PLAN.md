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
  with a lifetime, and nothing checks it: an unclosed handle leaks until the
  program ends, and only a read after `close_file` fails. Enforcing it would
  take a dedicated resource/ownership check (the value side of the language is
  otherwise unrestricted — see `DESIGN.md` §4). Until then the idiom is
  composition at the door: shadow `exit` with
  `select +i32 { status => { close_file(handle); status @ exit } }` where the
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

## Deferred, for discussion

- **The dual of effects.** `effect`/`handle` live entirely on the value side:
  a handled body returns a value, effect rows annotate `->`. What an
  effectful negative function or a handler on the consumer side means is an
  open question — to be discussed, not yet designed.

- **The additive units `0` and `⊤`.** Absent on both sides today — no
  variants to write, so no surface form. Whether they deserve one is a
  future discussion.

- **Composition syntax.** Composing a function with a continuation is
  already one binder away in either spelling — `select +A { x => f(x) @ k }`
  from the consumer side, `fn(x: +A) { f(x) @ k }` from the value side
  (`A → ⊥` *is* `-A`) — and negative functions compose by plain
  application. In a sequent calculus composition *is* the cut, so an
  operator would be a third name for it. Revisit once the stdlib work
  shows how often the eta-wrap recurs: the first remedy is a prelude
  function (`then(f, k)`), and surface syntax only if that proves
  insufficient.

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

The symmetry audit (after `menu` landed) left these queued, in order:

- **Variant imports: `use List::*;`.** Today `use` aliases a single module
  member, and enum variants come in bare through an *automatic* rule —
  unqualified while unambiguous — with a sharp edge: when a second
  declaration makes a bare name ambiguous, a pattern written with it
  silently degrades into a binder that catches everything. (The prelude's
  own list patterns hit exactly this and are fully qualified now.) The
  Rust-shaped fix is explicit variant imports — `use List::*;` and
  `use List::{Nil, Cons};` — where an imported name is *bound*, a collision
  is an error at the `use`, and a bare pattern name that resolves to
  nothing is an error rather than a catch-all. Whether the automatic rule
  then stays (Rust's prelude does auto-import `Some`/`None`) or every bare
  variant must be imported is the design decision to make first.

- **Test `impl Trait for Menu/Form`.** Trait dispatch was built against positive
  receivers; codata is where interfaces naturally live, so impls for menu
  types must be exercised and fixed or rejected with a real diagnostic.

- **Row-polymorphic effects.** Infer a function's effect row and let a
  higher-order function forward an argument's effects, so `map(f, xs)` can say
  it performs whatever `f` does — retiring the monomorphic-rows limit.

