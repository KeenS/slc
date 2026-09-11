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

## Deferred, for discussion

- **The dual of effects.** `effect`/`handle` live entirely on the value side:
  a handled body returns a value, effect rows annotate `->`. What an
  effectful negative function or a handler on the consumer side means is an
  open question — to be discussed, not yet designed.

- **The additive units `0` and `⊤`.** Absent on both sides today — no
  variants to write, so no surface form. Whether they deserve one is a
  future discussion.

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

- **Row-polymorphic effects.** Infer a function's effect row and let a
  higher-order function forward an argument's effects, so `map(f, xs)` can say
  it performs whatever `f` does — retiring the monomorphic-rows limit.

The symmetry audit (after `menu` landed) left these queued, in order:

- **Introduce the dual of `struct`.** The declaration square is ¾ complete:
  `enum` ↔ `menu`, `struct` ↔ nothing. The named ⅋ — a record of
  continuations supplied all at once — needs a declaration form, a
  construction form, and per-position access, the way `menu` got them for
  `&`. Naming to be settled (no anagram this time).

- **Make `mu` a pattern matcher, and enrich the patterns of both `select`
  and `mu`.** Pattern depth is one-sided: `match` has nesting, literals,
  guards, or-patterns; everything on the mirror side is flat. `mu` should
  bind its continuation by pattern the way `select` binds its scrutinee, and
  both should take the richer pattern forms (nested copatterns like
  `.tail(.head(out))`, wildcards, guards). Closing this also closes the
  eliminator gap: a branch table the core can express should lower to
  `μ̃[…]`/`μ[…]`, not to the `__match_dispatch` builtin.

- **Delete the exponential `!A`.** Linearity was eliminated — the core is
  classical, weakening and contraction are free — so `!` marks nothing. It
  also breaks the involution (`dual(!A) = !dual(A)` today, which is not the
  linear-logic `?dual(A)`, and no `?` exists). Remove the type rather than
  repair a modality the language no longer needs.

- **Define stdlib functions negatively where that is the simple form.** The
  prelude and builtins are all value-side. A function whose natural
  definition is a consumer transformer should be declared `<-`, so the
  negative half of the language has a standard library too.

- **Delete the builtin `[A]` and define lists in the prelude.** Lists are an
  ordinary recursive `enum`; the built-in type, literals, and list builtins
  should reduce to prelude definitions (and streams, their `menu` mirror,
  already need no builtin — `menu Stream { head: A, tail: Stream }` works
  today).

- **Test `impl Trait for Menu`.** Trait dispatch was built against positive
  receivers; codata is where interfaces naturally live, so impls for menu
  types must be exercised and fixed or rejected with a real diagnostic.
