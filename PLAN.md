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

The language is complete and the acceptance suite is green. Traits (ad-hoc
polymorphism) and algebraic effects with handlers are both shipped and
specified in `DESIGN.md`, and the execution model has been rebuilt around
them: the evaluator is an abstract machine over a closed, flat, de-Bruijn
instruction stream, its continuation first-class data (`DESIGN.md` §11). So
effect handlers are multi-shot, captured continuations are cheap and
reusable, and trait dispatch is resolved entirely at compile time.

No large feature is mid-flight. The open work is settling evaluation by
polarity: delayed computations still need to carry their effects.

## Known limits

### Of the design

- **`;` commutes only where one value meets one declared type.** `(A ; B)`
  and `(B ; A)` are one type, and a value is accepted at either spelling where
  it is cut into a consumer, stored in a record field, a variant or a
  `let`, passed as a written argument, or returned — the checker records a
  swap and lowering turns the closure around. Inside a type constructor —
  a tuple's component, `List<(A ; B)>` against `List<(B ; A)>` — there is no
  one value to turn, so the spelling still has to match; nor is an argument
  turned when it is the result of an earlier stage rather than a written
  value. Both would need the swap mapped through a structure or a chain.

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
  inference) is the known upgrade if these bite. A stage's row variables
  are instantiated only for the first stage of a chain, since that is the
  only one whose argument is syntax.

- **The file operations perform `IO` without an operation.** The `fs`
  module's `read`, `write`, `open`, `read_line`, `close` and `exists`, and
  the `__` primitives beneath them, charge `{IO}`, so their rows are honest,
  but they reach the outside world directly rather than by performing an
  operation the way `println` does — so they cannot be mocked by a handler.
  The outcome is not the obstacle; a reusable handler is. See "File
  operations as their own effect, and handlers as values". `DESIGN.md`'s
  `IO` section still gives the old reason, that an outcome needs a type the
  operation can name.

### Of the implementation

- **A run is capped at 1,000,000 machine steps.** The driver hands the
  machine that much fuel (`crates/slc-driver/src/main.rs`), so a plain loop
  of about 20,000 iterations stops with "evaluation diverged (fuel
  exhausted)". Tests that exercise long loops have to fit inside it.

## Next

### Evaluation

- **Delayed computations carry their effects.** Negative positions are by
  name (`DESIGN.md` §4, "When a `let` computes"), but the effect checker
  still charges a delayed computation's row where it is written, which is
  right only while the computation runs inside the declaration and under the
  handlers where it is written: one written inside a `handle` and run outside
  it performs its operation unhandled, a gap by-name evaluation opened.
  Decided: nothing is performed where a delayed computation is written — its
  row moves onto its type, and each use performs it, so the handler that must
  discharge it is the one around the use, the rule rowed menus and returned
  consumers (`-> (-A / {..E})`) already follow. Only a concrete row can ride
  on a type today, so until the rows-in-types upgrade that "Effect tracking
  follows names" names, a computation in a by-name position whose row is a
  variable is refused.

## Deferred, for discussion

- **File operations as their own effect, and handlers as values.** The
  `fs` module would declare an `Fs` effect, and a standard handler would
  answer it by performing `IO`. A program that touches files would then say
  `{Fs}` in its row, and a test could mock the file system with a handler of
  its own, the way `examples/io.sl` mocks output.

  The effect itself works today. An operation can take its outcome
  continuations as parameters, and a clause below its own prompt performs
  `IO` outward. This reads a file, and routes both outcomes, under an inline
  handler:

  ```sl
  effect Fs { fn read_file(path: String, ok: -String, failed: -String) -> (;); }

  handle mu String { k <= <("input.txt", k, select String { … }) | read_file } {
      read_file(path, ok, failed) => <path | __read_file | (ok & failed)>,
      return(s) => s,
  }
  ```

  What does not work is offering that handler for reuse. `handle` installs
  clauses only where it is written, so `fs` has nothing to export as "the
  real file system". A function that installs the handler around a lambda,
  `fn with_real_fs<+T, E>(body: ((,) -> T / {Fs, ..E})) -> T / {IO, ..E}`, is
  refused: the lambda's `Fs` is charged to the declaration that wrote it
  ("Effect tracking follows names"), so the caller is told it performs
  `Fs`. Eff answers this with first-class handlers — `handler { … }` is a
  value, and `with h handle c` installs it. What to discuss:

  - **The type of a handler value.** It has to say which effect it
    discharges, what its clauses perform in turn (`{Fs}` to `{IO}`), and how
    its return clause maps the body's result. That probably wants rows in
    types first, the upgrade "Effect tracking follows names" already names.
  - **Its place in the polarity story.** A handler consumes a computation,
    and its clauses bind continuations the copattern way, so it may be a
    negative value like a menu or a form rather than something new.
  - **Who installs the standard `Fs` handler.** `main` may leave only `IO`
    undischarged. So either the runtime installs this one too, making `Fs`
    a second special effect, or the driver wraps `main` in the prelude's
    handler, or every program installs it.

- **Composable capture.** A continuation that returns to where it was
  captured — `shift`'s `k : A -> R` — would be a function rather than a
  consumer, and would bring answer types into the checker, which the
  abortive, handler-delimited `mu` of `DESIGN.md` §6 keeps out — and it is
  what would give `reset` a use beyond refusing jumps. Revisit when a program
  needs one. The typing is worked out in Kobori, Kameyama and Kiselyov,
  "Answer-type modification without tears" (WoC 2015), and Materzok and
  Biernacki, "Subtyping delimited continuations" (ICFP 2011).
