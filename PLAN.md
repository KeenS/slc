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

  *Stage 2 — persistent continuation + indexed environments (cheap capture,
  faster lookup).* Represent the stack as a shared persistent cons so capture
  and `resume` are O(1) instead of cloning `Vec<Frame>`; compile variable
  access to de Bruijn indices over a flat environment instead of `HashMap`
  lookups with per-frame `Env` clones.

  *Stage 3 — compile the core to a closed IR (the raw speedup, and the home
  for deferred work).* Replace per-step `Rc<Term>` walking and cloning with a
  one-time compilation to a closure-converted, de-Bruijn instruction stream.
  That pass is also where the deferred **static dictionary passing for
  traits** lands (zero-cost dispatch, retiring the runtime type-key lookup),
  and where effect operations compile to efficient prompt instructions.

  The shape is the one the calculus already describes: the machine state is
  `⟨ term-closure ∥ coterm-closure ⟩`, and the coterm side *is* the
  continuation — one object. Completing that is simultaneously the
  correctness unlock (Stage 1), the cheap-capture change (Stage 2), and what
  a bytecode compiles against (Stage 3). The `fuel` counter stays — it is the
  divergence backstop the soundness story leans on.
