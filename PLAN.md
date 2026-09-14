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

No large feature is mid-flight. The work below is a sweep of the known
limits, in the order of "Next": the checker's remaining gap, type variables
without a polarity.

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
  unchecked, type variables carry no polarity kind, and the untyped evaluator
  remains the backstop for whatever that gap hides. The polarity kind is
  addressed by "Type variables carry a polarity"; the rest stays open.

## Next

The entries land in this order. Each runs its tests first, then the change,
then the documents, and passes `cargo fmt --check`, `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo test --workspace`
before it is committed.

### 1. Type variables carry a polarity

The known limit "Soundness is enforced by inference, argued informally"
names four gaps; this entry closes one. A generic parameter already states
its polarity (`<+T>`, `<-T>`), but an inference variable has none until it
is solved, so a variable used at both polarities in one declaration is
caught only if it is solved to something concrete, and otherwise left to the
evaluator.

1. **Tests first:** an unannotated lambda parameter used once as data and
   once as a consumer is refused; a `let` whose value's polarity is fixed by
   one use and contradicted by another is refused; every existing test and
   example still passes.
2. **Variables carry a kind.** Each unification variable holds a polarity —
   positive, negative or not yet known — set from a generic's mark, and from
   the polarity of whatever it is unified with; unifying a variable with a
   type of the other polarity, or two variables of different kinds, fails.
3. **Checks that guessed around variables decide by kind.** The places that
   skip a polarity check because a type still has a variable — the
   orientation rule in the flow arm's `!contains_var(&acc)`, and the
   `pending_lets` refusal "whose polarity is not known" — read the kind
   instead, and each change gets a test.
4. **Docs.** `DESIGN.md` §4 "Polarity by position" says variables carry the
   polarity; this file removes that gap from the known limit's list.

## Deferred, for discussion

- **Handler values.** A handler is an ordinary function today, taking the
  computation it handles (`fs::real`). A first-class `handler { … }` that a
  program stores, chooses between or composes, installed with
  `with h handle c`, needs a type for its handled effects, its input and
  output types and its clauses' row. Revisit when a program needs a handler
  as data rather than as a function
  (`docs/design-notes/file-system-effect.md`).

- **Row variables on declarations.** A menu or form declaration keeps a
  concrete row: `menu Seq<+T, E> / {..E}` would
  instantiate its row like a type parameter at each use. Revisit when a
  per-use row on the type, `(Seq<B> / {..E})`, is not enough.

- **The stdlib's lazy codata carries its rows on the returned type.**
  `seq::map`, `filter` and `take_while` can declare `-> (Seq<B> / {..E})`
  instead of a call row, since building a `Seq` performs nothing; with
  `Step::Yield`'s payload typed `(Seq<T> / {..E})` in turn, their bodies lose
  the `let+` that a payload declared without a row requires.

- **Composable capture.** A continuation that returns to where it was
  captured — `shift`'s `k : A -> R` — would be a function rather than a
  consumer, and would bring answer types into the checker, which the
  abortive, handler-delimited `mu` of `DESIGN.md` §6 keeps out — and it is
  what would give `reset` a use beyond refusing jumps. Revisit when a program
  needs one. The typing is worked out in Kobori, Kameyama and Kiselyov,
  "Answer-type modification without tears" (WoC 2015), and Materzok and
  Biernacki, "Subtyping delimited continuations" (ICFP 2011).
