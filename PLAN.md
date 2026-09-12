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

- **No partial application.** `f(a)` of a two-parameter `f` does not
  produce a function awaiting the second argument: the call's type is the
  full result, so `7 @ route("high")` is refused even though
  `route(tag, x) -> ⊥` would make the partial application a consumer of
  `+i64`. Calls are curried in the core and the runtime accumulates
  arguments, so this is a checker-side gap — the type of an
  under-applied call — not a representational one. (Found while
  establishing that operations need no negative form.)

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

- **Effect tracking follows names.** Rows and row variables are explicit
  and checked per declaration, but the rows live beside the type system
  rather than in core types: a lambda's effects are charged where it is
  written, a higher-order global passed as a value forwards nothing
  further, and a function laundered through a `let` binding is not
  tracked. Moving rows into the arrow type itself (unified during
  inference) is the known upgrade if these bite.

## Next

- **Irrefutable patterns at binders, as in Rust.** The one box of the
  composition redesign still open; everything else in it has shipped.
  `let p = e`, a parameter `p: T`, and a `mu` binder arm `p <= c` take a
  pattern, not just a name. A header *is* such a pattern with typed
  leaves — the value group a tuple pattern on the one argument, the
  continuation group a bundle pattern on the one exit-menu — which is
  what the unary calling convention already stands on, with plain names
  where patterns belong.

  - [ ] `Expr::Let { name }` and `Param { name }` become patterns; a bare
        name is the trivial one. The bundle copattern `(p & q)` and the
        nullary `(,)`/`(&)` patterns already parse, so the grammar work is
        in the binder positions.
  - [ ] Refutability: a binder pattern must be exhaustive for its type —
        reuse `exhaustive.rs` — so tuples, records, single-variant enums,
        wildcards, and bundles pass, and a many-variant enum is refused
        with a pointer at `match`.
  - [ ] Lowering: `let p = e; rest` is the one-arm match, a parameter
        pattern a match on the incoming argument (`bind_group` already
        destructures a group; this generalises its binders), a bundle
        pattern its projections. `let … else` stays out of scope.
  - [ ] Nested patterns at every leaf: `fn f((a, b): (i64, i64), c: String)`.
        Example and DESIGN note; MIGRATION needs nothing, since names stay
        the trivial pattern.

- **Retire `Expr::Cut`.** Nothing constructs it now that `@` is gone —
  a closed flow is the cut — but the node and its arms remain in the AST,
  the checker, the effect checker, and lowering. Delete it, or keep it as
  the internal representation a closed chain lowers *through* rather than
  a parallel form.

## Deferred, for discussion

- **The flow operator's two ambiguities, resolved by preference rather
  than by rule.** `|` reads a step from the types at its ends, and twice
  one type admits two readings. Both work today, decided by trying a
  reading against a *copy* of the unification state (`would_fit` in
  `crates/slc-check/src/expr.rs`), which costs nothing and commits
  nothing — but where both readings fit, the winner is a preference no
  one has agreed to, and nothing says a collision happened.

  1. **A `Par` is a function and a consumer.** `A → B` is `-A ⅋ B`, and
     `-A ⅋ -B` is `dual(A ⊗ B)`, so a stage of `Par` type may apply or
     may consume a product: `21 | double` applies, `(7, "x") |
     report_first(k)` cuts. *Preference:* the function reading is tried
     first, and the consumer reading catches what it misses.

  2. **A function is itself a value.** `f | k` composes when `k` takes
     what `f` returns, and is the cut that sends `f` to `k` when `k`
     takes `f` — `fn(x) { x } | k` in `examples/polymorphism.sl` means
     the latter. *Preference:* the opposite one — a chain opens with a
     function only when the next stage will **not** accept it as a
     value, so the cut wins where both fit.

  The two preferences point opposite ways, which is worth either a
  reason or a change. Two cases make it concrete: a consumer whose type
  is still unsolved takes the cut reading, so `f | k` with an
  undetermined `k` never composes; and a function whose result has its
  own type — `f: (A → A) → (A → A)` against `k: -(A → A)` — fits both
  readings exactly, and silently cuts. To settle: whether the
  preferences are the right defaults and get written into DESIGN as
  rules, whether a collision should be reported rather than resolved,
  and whether either reading deserves a way to say which was meant.

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
