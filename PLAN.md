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

Nothing is queued. The next entries come from "Deferred" below, once one is
chosen.

## Deferred, for discussion

- **A returned consumer's row is charged to the declaration, not the value.**
  `fn mk(exit: -i32) -> -i64 / {Tick}` absorbs what the returned `select`
  performs into the call's row (`crates/slc-check/src/expr.rs`, the
  declaration-return charge; `docs/design-notes/rows-in-types.md`), so a
  `handle` around the call discharges nothing and the consumer fires later
  unhandled: `let k = handle (<exit | mk) { tick(): … }; <5 | k>` runs
  `tick` with no handler. The sound spelling is `-> (-i64 / {Tick})`. Decide
  whether to refuse the rowless promise ("the returned value performs
  `Tick`; write `-> (-i64 / {Tick})`") or keep the charge for values that are
  run before the call returns (a rowless menu built by a function). Refusing
  is the recommendation.

- **A handler clause's parameter count is not checked.** A clause takes
  parameter *i* from the operation's signature and gives any extra one a
  fresh variable (`Expr::Handle` in `crates/slc-check/src/expr.rs`);
  `fs::write_file(p): resume => …` types `p` as `String` while the runtime
  binds the `(path, contents)` tuple, and `read_file(a, b)` aborts at run
  time. Decide whether a nullary operation's clause may still write one
  ignored binder, `config(u)`, then refuse every other count by name:
  "`write_file` takes 2 parameters, and this clause binds 1".

- **A handler naming one operation discharges the whole effect.** The
  handled set is the effects of the operations the clauses name
  (`Expr::Handle`), so a mock answering only `fs::read_file` type-checks a
  program that calls `fs::write` and aborts with "no handler for operation
  fs::write_file" — against DESIGN's "a well-typed program performs no
  operation the runtime cannot answer". Either a handler must name every
  operation of each effect it handles (a clause per operation, or a
  forwarding default), or the row tracks operations rather than effects.
  The first is the smaller change.

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
