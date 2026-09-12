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

The queue came from an audit of the polarity×feature matrix (traits and
effects against negative functions), in order of depth. Every item ends the
same way, unlisted: `cargo fmt`, `cargo clippy --workspace --all-targets --
-D warnings`, `cargo test --workspace` (19 suites green), the examples loop
(every `examples/*.sl` runs; `*_error.sl` and `command_falls_through.sl`
must fail), DESIGN.md updated where behaviour changed, and the entry
retired from this file.

## Deferred, for discussion

- **Surface syntax for the additive units `0` and `⊤`.** Their explicit
  prelude forms now exist: `enum Empty {}` is eliminated by `select Empty {}`,
  and the unique value of `menu Top {}` is `mu Top {}`. What remains open is
  whether `Empty` and `Top` should gain symbolic or otherwise concise type and
  term aliases, analogous to `()`/`Unit` and `⊥`/`Bottom`; settle those
  spellings later rather than reserving syntax now.

- **Composition syntax: the pipeline cut.** Composing a function with a
  continuation is one binder away in either spelling today —
  `select +A { x => f(x) @ k }` or `fn(x: +A) { f(x) @ k }` — and the
  prelude has `then(f, k)`. The candidate surface syntax under
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
