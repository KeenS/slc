# Slant: the standing plan

Slant is a Rust-flavoured surface over a classical λ̄μμ̃ core. The redesign
that established it is finished and recorded in `docs/HISTORY.md`; the
language itself is defined in `DESIGN.md`. This file is neither — it holds
only what is still open: the known limits, the work queued next, and what is
deferred for discussion.

The contract that keeps it short: a decision, once made, goes to `DESIGN.md`
and leaves this file. The plan is where work stops being open, not where it
is remembered — so an entry here is a promise still outstanding, and nothing
else belongs. Entries refer to each other by name, never by position.

## Status

The acceptance suite covers the corrected runtime and effect rules as well
as complete design programs. Traits (ad-hoc
polymorphism) and algebraic effects with handlers are both shipped and
specified in `DESIGN.md`, and the execution model has been rebuilt around
them: the evaluator is an abstract machine over a closed, flat, de-Bruijn
instruction stream, its continuation first-class data (`DESIGN.md` §11). So
effect handlers are multi-shot, captured continuations are cheap and
reusable, and trait dispatch is resolved entirely at compile time.

The queue the redesign approved is complete. Generic effects, composable
capture, and structural stage adapters are specified in `DESIGN.md`, with
runnable examples and regression coverage. The known design limits below
remain. What `Deferred` holds now is what writing programs in the language
turned up — the formatter, the editor mode, and the examples under
`examples/programs/` — rather than anything the redesign left undone. The
part of it that needed no decision is built: located syntax errors,
`slc check`, several bounds on a type parameter, and programs of more than
one file are in `DESIGN.md`.

Call-by-name is the settled direction for delayed computation: every demand
runs it afresh under the handlers around that demand. Future changes must keep
the polarity-directed evaluation discipline and explicit eager evaluation;
they do not replace it with a general call-by-value default. Call-by-need,
memoized thunks and implicit result caching are out of scope, not deferred
features. New features must preserve the separate forcing and activation
rows and the demand-time handler boundaries specified in `DESIGN.md`.

## Known limits

### Of the design

- **Structural adapters require a finite derivation.** Lifting through
  available declarations supports stored structures and regular recursion,
  but not recursive specialization with growing type arguments. Derivation
  is bounded, and opaque constructors still require matching arguments.
  There is no user-defined lifting interface for an opaque constructor, nor
  unrestricted commutative type equality. Capability rows are not mapped.

- **Stored handlers discharge concrete capabilities.** `Handler<A, B, E, F>`
  preserves both rows, but installation discharges only effects explicitly
  present in `E`; an unknown handled-row tail does not grant capabilities.
  Abstraction over unknown capability tails remains unsupported; typed
  generic applications inside concrete capabilities are supported.

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, and the untyped evaluator remains the backstop for whatever that
  gap hides. A type variable does carry the polarity of the generic parameters
  it meets (`DESIGN.md` §4, "Polarity by position").

## Next

Each entry starts with regressions, then implementation and documentation,
and passes `cargo fmt --check`, `cargo clippy --workspace --all-targets --
-D warnings` and `cargo test --workspace` before it is considered complete.
New or changed syntax also needs runnable examples with exact-output tests;
compiling complete `DESIGN.md` programs alone does not establish their
runtime behaviour.

No queued implementation tasks.

## Deferred, for discussion

Each of these needs a decision before it is work. Once one is made it goes
to `DESIGN.md`, and whatever it leaves to build moves up to `Next`.

- **A REPL.** Feasible, and what it turns on is settled by two decisions
  rather than by work. The work is known: values kept between entries index
  compiled code, which today is one fresh chunk per program, so the chunk
  must grow append-only; and the checker checks whole programs, so it needs
  an entry point for one entry against the declarations so far and the types
  of earlier bindings.

  The first decision is what a continuation captured in one entry means in a
  later one. Nothing outlives its machine run today — a `const` is a
  literal — so the prompt is where this first arises. Read off the jump code
  and not yet run: entries run under the runtime's `IO` handler as `main`
  does, each installation has its own prompt, and a jump that first meets a
  prompt its target does not hold is refused (`ForeignPrompt`) — the rule
  `examples/errors/delimited_error.sl` shows for `reset`. So a stale
  continuation is refused with no new code. The alternative is Scheme's: the
  jump re-enters the earlier entry, and what is left of it becomes the
  current entry's result. Refusing is the rule the language already has.

  The second is what an entry is: a value is printed through `Display`, a
  command runs and prints nothing, `exit` leaves the REPL with its status,
  and a later definition shadows an earlier one — where the driver's rule for
  the prelude is that the first wins. And what `let` means at the prompt:
  keep the value, which needs the work above, or re-evaluate it where it is
  used, which needs none and repeats its effects.

  A first version can decide only the second: keep the session as source,
  recompile it whole for each entry, and re-evaluate `let`. Nothing persists
  at run time, so no continuation crosses entries. An entry left incomplete
  is already recognisable — its parse error is at the end of input, with no
  extent — and `--fuel` bounds one that does not stop.

- **What traits still lack.** Default methods, associated types, and
  supertraits. Each is ordinary in Rust, whose flavour the surface keeps;
  whether each earns its place here, given that dispatch is resolved
  entirely at compile time, is undecided.


