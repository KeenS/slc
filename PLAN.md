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

## Deferred, with no accepted replacement

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

- **Evolve the execution model: one continuation, then compile to it.** The
  machine reifies the continuation two ways at once — control (match, the
  effect `Prompt`) lives in the frame stack, but application, `let`, and
  blocks lower to `μk. ⟨… ∥ k⟩`, which the machine turns into a
  stack-*replacing* `Value::Kont`. That split is the root cause of the
  tail-resumptive effect limit: the continuation between a `perform` and its
  handler is part frames, part escaping `Kont`. Healing it addresses all
  three goals at once.

  *Stage 1 — unify the continuation. **Done.*** `let`, blocks, and `if`
  routed their result through a μ-captured covariable that the machine turned
  into a stack-replacing `Kont`; they now route to an unbound covar the
  machine delivers to the current frame stack. A handler's `resume` is a
  first-class slice of that one stack, so it composes and repeats freely —
  multi-shot handlers (nondeterminism) work, and the restriction is gone
  (`examples/effects.sl`). Application's `__call` was already framewise (the
  arg-shortcut ignores it).

  *Stage 2 — persistent continuation + indexed environments. **Done.***
  The continuation is a shared cons (`machine::Kont`) with the top at the
  head, so `mu`'s `Value::Kont` and a handler's `resume` capture by cloning
  one `Rc` — O(1) however deep, and a pushed frame never disturbs a stack
  already captured, so a resumed continuation walks its own copy (multi-shot
  now holds by construction, not by `Vec` cloning). The environment is
  likewise a cons — O(1) to clone per step — and variable access is a de
  Bruijn index into its positional chain, no `HashMap` walk (see Stage 3).

  *Stage 3 — compile the core to a closed IR. **Core done; two follow-ons
  remain.*** The machine no longer walks the named core: a compile pass
  (`slc-runtime/src/compile.rs`) resolves every lexical binder once to a de
  Bruijn index, producing a closed IR (`ir.rs`) the machine runs. Variable
  and co-variable references are `Local`/`CoLocal` indices into one
  positional environment (they share it — a co-variable binds to a consumer
  value); globals, literals, and `match` pattern variables stay `Dynamic`
  names, the last injected into a by-name overlay that leaves the indices
  stable. `Value`'s code-bearing variants carry IR and dropped their binder
  names.

  **Static dispatch — monomorphic calls done.** The checker resolves each
  trait-method call whose receiver type is concrete to its impl
  (`check_program_resolving` returns a span→impl map), and lowering turns
  that call into a direct call to the impl — no runtime `type_key` lookup.
  A call whose receiver is a rigid `<T: Trait>` parameter stays dynamic
  (the `Value::Method` path).

  Remaining:
  - **Dictionary passing for the polymorphic case** — so a bounded function
    `fn f<T: Trait>(…)` receives its trait dictionary as a hidden argument
    and dispatches statically inside too, retiring `Value::Method` entirely.
    Larger: it adds dictionary parameters in lowering and threads them from
    every call site.
  - **Linearize the IR to a flat instruction stream** (`Op::Local(u16)`,
    `Op::CallDict`, `Op::Prompt`, …) run by an explicit instruction pointer
    instead of the tree-walk. Mostly mechanical on top of the closed IR; the
    headline wins (no name resolution, O(1) env/continuation capture) are
    already banked, so this is a smaller, later speedup.

  The shape is the one the calculus already describes: the machine state is
  `⟨ term-closure ∥ coterm-closure ⟩`, and the coterm side *is* the
  continuation — one object. Completing that is simultaneously the
  correctness unlock (Stage 1), the cheap-capture change (Stage 2), and what
  a bytecode compiles against (Stage 3). The `fuel` counter stays — it is the
  divergence backstop the soundness story leans on.
