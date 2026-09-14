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

No large feature is mid-flight. The sweep of the known limits has landed;
"Next" holds the inconveniences writing programs against it turned up, each
small, cheapest first.

## Known limits

### Of the design

- **`;` commutes only where there is a value to turn around.** `(A ; B)`
  and `(B ; A)` are one type, and a value is accepted at either spelling where
  one value meets one declared type — cut into a consumer, stored, passed,
  returned, written as a tuple's component or an alternative, or handed from
  one stage to the next — and lowering turns the closure around. A type
  constructor's arguments, `List<(A ; B)>` against `List<(B ; A)>`, and a
  structure that is not written out, have no one value to turn, so the
  spelling still has to match; mapping a swap through them would need a
  traversal per declaration.

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, and the untyped evaluator remains the backstop for whatever that
  gap hides. A type variable does carry the polarity of the generic parameters
  it meets (`DESIGN.md` §4, "Polarity by position").

## Next

The entries land in this order. Each runs its tests first, then the change,
then the documents, and passes `cargo fmt --check`, `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo test --workspace`
before it is committed. Where an entry says "Proposed", the choice is
confirmed before the change.

### 7. A menu or form declaration takes a row variable

Moved up from "Deferred": the stdlib's lazy codata hit its stop condition.
`seq::map` declared `-> (Seq<B> / {..E})`, storing `<(f, rest) | map` directly,
is refused — "`map` hands on a value that performs the row `..E` where the
type it meets does not allow it" — because `Step::Yield(T, Seq<T>)` declares
its payload without a row, and `seq::to_list(s: Seq<T>)` takes a pure one.
Building the rowed rest without storing it is accepted, so the payload is the
blocker, and a per-use row on the type is not enough.

Proposed: a declaration names a row parameter as it names a type parameter —
`menu Seq<+T, E> / {..E}`, `enum Step<+T, E> { Done, Yield(T, Seq<T, E>) }` —
and each use instantiates it. How a use writes the argument (`Seq<T, ..E>`,
`Seq<T, {IO}>`) is decided in this step.

1. **Tests first:** `seq::map` storing its rest directly is accepted; a `Seq`
   built by `seq::map` with an effectful function and demanded under a
   handler is accepted, and demanded outside one is refused;
   `examples/seq.sl` and the stdlib tests keep their output.
2. **The checker.** A declaration records its row parameters beside its type
   parameters; a use instantiates them fresh, and a menu's latent row is its
   row argument.
3. **The stdlib.** `seq::map`, `filter` and `take_while` store their rest
   directly and `let+` goes; `to_list` and `take` forward the row.
4. **Docs.** `DESIGN.md`'s declarations and stdlib sections, the comments in
   `seq.sl` and `fs.sl`, and `MIGRATION.md`.

## Deferred, for discussion

- **Value-producing `select` arms.** A `select` arm is a command, so a
  handler clause routing a primitive's outcomes back captures its own result
  with `mu { out <= … <(<v | resume) | out> … }`, as `fs::real` does four
  times. Letting an arm be an expression would make a value-producing
  `select` a consumer of `A` producing `B` — a function `(A -> B)`, or a type
  of its own. Revisit when more code than `fs::real` needs it.

- **Handler values.** A handler is an ordinary function today, taking the
  computation it handles (`fs::real`). A first-class `handler { … }` that a
  program stores, chooses between or composes, installed with
  `with h handle c`, needs a type for its handled effects, its input and
  output types and its clauses' row. Revisit when a program needs a handler
  as data rather than as a function
  (`docs/design-notes/file-system-effect.md`).

- **Composable capture.** A continuation that returns to where it was
  captured — `shift`'s `k : A -> R` — would be a function rather than a
  consumer, and would bring answer types into the checker, which the
  abortive, handler-delimited `mu` of `DESIGN.md` §6 keeps out — and it is
  what would give `reset` a use beyond refusing jumps. Revisit when a program
  needs one. The typing is worked out in Kobori, Kameyama and Kiselyov,
  "Answer-type modification without tears" (WoC 2015), and Materzok and
  Biernacki, "Subtyping delimited continuations" (ICFP 2011).
