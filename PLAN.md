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

- **Composition syntax: the pipeline cut.** Composing a function with a
  continuation is one binder away in either spelling today —
  `select +A { x => f(x) @ k }` or `fn(x: +A) { f(x) @ k }` — and the
  prelude has `then(f, ↓k)`. The candidate surface syntax under
  consideration writes the cut as a pipeline, values flowing left through
  functions into continuations:

  ```sl
  <(v1, v2) | f1 | f2 | (k1, k2)>
  ```

  — a surface spelling of the core's own `⟨ t ∥ e ⟩`, with the composition
  chain written between the sides. The half-open fragments would then
  denote the sides themselves:

  ```sl
  <(v1, v2,) | f1 |        // a producer: values piped through f1,
                           // awaiting its continuation
  | f2 | (k1, k2)>         // a consumer: pipe through f2, deliver to
                           // the row — composition of f2 with k, as syntax
  ```

  To settle before adopting: the meaning of a middle stage (positive
  functions apply; negative functions and multi-outcome commands would
  consume the rest of the pipe as their row), whether `(v1, v2)` packs a
  tensor or spreads arguments and `(k1, k2)` is a row or a single `⅋`
  consumer, how the half-open forms type (term and co-term of the same
  pipeline), and the grammar itself — `<` opens type arguments and `|`
  separates a `command`'s groups, so both need disambiguation in
  expression position.

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

- **Test `impl Trait for Menu/Form`.** Trait dispatch was built against positive
  receivers; codata is where interfaces naturally live, so impls for menu
  types must be exercised and fixed or rejected with a real diagnostic.

- **A negative-side prelude: useful `menu`s and `form`s beyond `Stream`.**
  The prelude's declarations are all positive (`List<T>`); the negative
  column deserves residents of its own. Candidates to weigh:
  `menu Stream<T> { head: T, tail: Stream<T> }` — the coinductive mirror of
  `List`, already expressible (`examples/stream.sl`) — with `take`
  (`Stream<T>` to `List<T>`), a stream `map`, and `from`/`repeat`
  constructors beside it; `menu Lazy<T> { force: T }` — a one-item menu is
  a by-name thunk, re-demanded per use; and a named `form` for the
  recurring ok/err consumer pair, so multi-outcome pipelines can pass one
  value instead of an ad-hoc pair of continuations. To settle while
  choosing: which of these earn residence, what the naming conventions for
  demands are, and how `Display` meets codata — an infinite `Stream` cannot
  print whole, so `fmt(take(s, n))` may be the honest form rather than an
  `impl Display for Stream`.

- **Row-polymorphic effects.** Infer a function's effect row and let a
  higher-order function forward an argument's effects, so `map(f, xs)` can say
  it performs whatever `f` does — retiring the monomorphic-rows limit.

