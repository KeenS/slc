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
limits, in the order of "Next": the step cap first, then effects — delayed
computations, rows in types, and the file system as an effect that rows in
types unblocks — and then the checker's remaining gaps.

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
  Addressed by "`;` commutes through structures and stages".

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, type variables carry no polarity kind, and the untyped evaluator
  remains the backstop for whatever that gap hides. The polarity kind is
  addressed by "Type variables carry a polarity"; the rest stays open.

- **Effect tracking follows names.** Rows and row variables are explicit
  and checked per declaration, but the rows live beside the type system
  rather than in core types: a lambda's effects are charged where it is
  written, a higher-order global passed as a value forwards nothing
  further, and a function laundered through a `let` binding is not
  tracked. Moving rows into the arrow type itself (unified during
  inference) is the known upgrade if these bite. A stage's row variables
  are instantiated only for the first stage of a chain, since that is the
  only one whose argument is syntax. Addressed by "Rows live in types".

- **The file operations perform `IO` without an operation.** The `fs`
  module's `read`, `write`, `open`, `read_line`, `close` and `exists`, and
  the `__` primitives beneath them, charge `{IO}`, so their rows are honest,
  but they reach the outside world directly rather than by performing an
  operation the way `println` does — so they cannot be mocked by a handler.
  The outcome is not the obstacle; a reusable handler is. `DESIGN.md`'s
  `IO` section still gives the old reason, that an outcome needs a type the
  operation can name. Addressed by "File operations are an effect, and
  handlers are values".

### Of the implementation

- **A run is capped at 1,000,000 machine steps.** The driver hands the
  machine that much fuel (`crates/slc-driver/src/main.rs`), so a plain loop
  of about 20,000 iterations stops with "evaluation diverged (fuel
  exhausted)". Tests that exercise long loops have to fit inside it.
  Addressed by "A run has no step cap unless one is asked for".

## Next

The entries land in this order. Each runs its tests first, then the change,
then the documents, and passes `cargo fmt --check`, `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo test --workspace`
before it is committed.

### 1. A run has no step cap unless one is asked for

Decided: the cap was a guard against runaway evaluation from before tail
cuts ran in constant space (`DESIGN.md` §11). Now a loop is bounded only by
memory, so `slc run` gives no cap, and `slc run --fuel N file.sl` keeps one
for tests and for bounding a run that might diverge. The runtime keeps
counting steps: no cap is `usize::MAX` fuel.

1. **Tests first,** in `crates/slc-driver/tests/integration.rs`:
   - a tail-recursive loop of 100,000 iterations prints its answer;
   - a tail-resuming handler around 100,000 operations prints its answer;
   - `slc run --fuel 1000` on a loop stops with "evaluation diverged (fuel
     exhausted)" and a failing status;
   - `slc run --fuel` without a number, and `--fuel x`, report usage.
2. **The driver.** `main.rs` parses `run [--fuel N] <file.sl>`; the three
   `let mut fuel = 1_000_000;` sites — global initializers, `main`'s root,
   and the entry — take the one budget, so global initialization and the
   run share it. The usage line names the flag.
3. **Docs.**
   - `DESIGN.md` §11 "Execution": "a fuel bound turns divergence into an
     error" becomes the opt-in flag.
   - The driver's usage line is the flag's only other documentation; no
     document describes `slc run` today.
   - This file: remove the known limit "A run is capped at 1,000,000 machine
     steps".

### 2. Delayed computations carry their effects

Negative positions are by name (`DESIGN.md` §4, "When a `let` computes"), but
the effect checker still charges a delayed computation's row where it is
written, which is right only while the computation runs inside the
declaration and under the handlers where it is written: one written inside a
`handle` and run outside it performs its operation unhandled, a gap by-name
evaluation opened.

Decided: nothing is performed where a delayed computation is written — its
row moves onto its value, and each use performs it, so the handler that must
discharge it is the one around the use, the rule rowed menus and returned
consumers (`-> (-A / {..E})`) already follow. Only a concrete row can ride on
a value today, so until "Rows live in types" a computation in a by-name
position whose row has a variable is refused.

What the passes have to work with: the type checker settles which spans are
delayed and records them in `dispatch.delays`
(`crates/slc-syntax/src/lower.rs`, the `DELAYS` table lowering reads through
`delay_if_delayed`); the effect checker (`crates/slc-check/src/effects.rs`)
runs after it in the driver but walks names without that table.

1. **Tests first,** in `crates/slc-check/src/effects.rs` for diagnostics and
   `crates/slc-driver/tests/integration.rs` for runs:
   - `let- f = { <"x" | throw; fn(n: i64) { n } }` written inside a `handle`
     for `throw` and demanded outside it is refused: the demand performs
     `Exn` unhandled;
   - the same binding written outside and demanded inside the handler is
     accepted, and the handler answers at run time;
   - `let+` of the same computation inside the handler is charged there,
     as today;
   - a delayed computation whose row is `{..E}` is refused, and the message
     suggests `let+`;
   - each by-name position is covered: a `let`, what flows into a chain, a
     parenthesised argument, a tuple component and a bundle item.
2. **The effect checker learns which spans are delayed.**
   `check_effects(&program)` becomes `check_effects(&program, &delays)`,
   and the driver passes the table the type checker produced.
3. **A delayed span charges nothing where it is written.** In `collect`, the
   row of a delayed expression is gathered into its own `Row` instead of
   `out`, and becomes the latent row of what holds it:
   - a `let` binder records it in `ctx.locals`, which `charge_cut` already
     reads, and a call or projection on that name charges it too
     (`row_of_name`, `Expr::Project`);
   - a delayed argument is checked against its parameter's declared latent
     row where there is one, the way `charge_call` treats a rowed function
     argument, and is charged at the call where there is none, since the
     callee runs it inside the call;
   - a delayed tuple component or bundle item is charged where the tuple or
     bundle is taken apart or consumed; where names cannot follow it, it is
     charged where it is written, and the test for that position says so.
4. **The refusal.** A delayed row with a tail is reported at the delayed
   span, naming the variable and suggesting `let+` or an annotation.
5. **Docs.**
   - `DESIGN.md` §4 "When a `let` computes": a delayed computation's
     effects happen at each use, under the handlers there.
   - The effects section: delayed computations join menus and returned
     consumers as values that carry a row.
   - `MIGRATION.md`: a program that relied on a handler around the binding
     now puts the handler around the use or writes `let+`.
   - Run every example; any whose output changes gets its handler moved.

### 3. Rows live in types

Decided: rows move from the name-following pass into inferred types, the
upgrade "Effect tracking follows names" names. What does not change is the
surface: rows are written where they are today, `/ {E, ..R}` after an
arrow and on menus and forms.

The core has no arrow type — `A -> B` is `(-A ; B)` — so a row attaches to
the negative type that runs: a function, a consumer, a menu or a form. The
first step settles the representation before anything is ported.

1. **Design note.** `docs/design-notes/rows-in-types.md` settles:
   - the representation, a row on `Type::Par` and `Type::With` against a
     `Type::Rowed(Box<Type>, Row)` wrapper, by which one keeps `dual` an
     involution and `;` commutative;
   - row unification: effect labels as a set with an optional tail variable
     (Rémy-style), and how a tail variable unifies with a concrete row;
   - where a row is instantiated and generalized, beside a declaration's
     type variables;
   - what `handle` does to a row: remove the handled effect's label, and
     leave a tail alone.
   The note is reviewed before step 2.
2. **Tests first.** One checker test for each gap "Effect tracking follows
   names" lists — a lambda's effects charged where it runs, a higher-order
   global passed as a value forwarding its row, a function laundered through
   a `let` still charging its row, and a row variable instantiated at a later
   stage of a chain — plus the whole of `effects.rs`'s existing test module,
   which must keep passing unchanged.
3. **Types carry rows.** `crates/slc-core/src/types.rs` and unification
   (`slc_core::typing::Unification`) gain rows; `display` prints them in the
   surface spelling; `dual` and the commutation of `;` keep them.
4. **The type checker infers rows.** Signatures lower their written rows
   into types (`crates/slc-check/src/signatures.rs`); a call, a stage, a cut
   and a demand add the callee's row to the declaration's; `handle` removes
   what it handles; each declaration's inferred row is checked against its
   written one.
5. **The name-following pass is retired.** `effects.rs` keeps only what
   types cannot say, if anything, and its diagnostics move to the type
   checker with the same messages. "Delayed computations carry their
   effects" loses its refusal of row variables.
6. **Docs.** `DESIGN.md`'s effects section describes rows as part of types;
   this file removes the known limit "Effect tracking follows names";
   `MIGRATION.md` gains a section only for programs whose meaning or
   acceptance changes.

### 4. File operations are an effect, and handlers are values

Needs "Rows live in types". The `fs` module would declare an `Fs` effect,
and a standard handler would answer it by performing `IO`. A program that
touches files would then say `{Fs}` in its row, and a test could mock the
file system with a handler of its own, the way `examples/io.sl` mocks output.

The effect itself works today. An operation can take its outcome
continuations as parameters, and a clause below its own prompt performs `IO`
outward:

```sl
effect Fs { fn read_file(path: String, ok: -String, failed: -String) -> (;); }

handle mu String { k <= <("input.txt", k, select String { … }) | read_file } {
    read_file(path, ok, failed) => <path | __read_file | (ok & failed)>,
    return(s) => s,
}
```

What does not work is offering that handler for reuse: `handle` installs
clauses only where it is written, so `fs` has nothing to export as "the real
file system". Eff answers this with first-class handlers — `handler { … }`
is a value, and `with h handle c` installs it.

1. **Design note.** `docs/design-notes/handler-values.md` answers:
   - the type of a handler value: the effect it discharges, what its clauses
     perform in turn (`{Fs}` to `{IO}`), and how its return clause maps the
     body's result, in the row representation "Rows live in types" chose;
   - its place in the polarity story: a handler consumes a computation and
     binds continuations the copattern way, so whether it is a negative
     value like a menu or a form;
   - who installs the standard `Fs` handler, given that `main` may leave
     only `IO` undischarged: the runtime (a second special effect), the
     driver wrapping `main` in the prelude's handler, or every program.
   The note is reviewed before step 2.
2. **Tests first:** a handler value bound by `let` and installed twice
   answers both times; a handler passed to a function and installed there
   discharges its effect from the caller's row; `examples/file_io.sl`'s
   output is unchanged; a test reads a file through a mock handler that
   never touches the disk.
3. **Handler values.** Parser, lowering (a handler value is the clause table
   `__handle` already takes, unapplied), checker and runtime.
4. **`Fs`.** The `fs` module declares the effect and exports the standard
   handler; `read`, `write`, `open`, `read_line`, `close` and `exists`
   perform its operations; the installer chosen in step 1 is put in place.
5. **Docs.** `DESIGN.md`'s effects and `IO` sections — the stale reason in
   the latter goes; `MIGRATION.md` for rows that now say `{Fs}`; this file
   removes the known limit "The file operations perform `IO` without an
   operation".

### 5. `;` commutes through structures and stages

The known limit "`;` commutes only where one value meets one declared type"
has two halves, and a third gap of the same family turned up beside it.

Decided: a swap maps through a structure whose components are written out —
a tuple, an anonymous sum — by turning each component, and through a stage's
result by turning the value between stages. A named type constructor's
arguments, `List<(A ; B)>`, are not mapped: that needs a traversal per
declaration, so the spelling still has to match, and the diagnostic says so.

1. **Tests first,** in `crates/slc-check/src/expr.rs` and
   `crates/slc-driver/tests/integration.rs`:
   - a tuple whose component is written at the mirrored spelling of its
     declared component type is accepted and runs;
   - a sum alternative likewise;
   - `<x | f | g>` where `f`'s result meets `g`'s parameter at the mirrored
     spelling is accepted and runs;
   - `List<(A ; B)>` against `List<(B ; A)>` is refused, with a message
     naming the type constructor.
2. **Structures.** `commute` (`expr.rs`) returns a swap per component, and
   `Swap` gains a component form that lowering (`swap_adapter`,
   `crates/slc-syntax/src/lower.rs`) applies by taking the tuple or sum
   apart, turning the component and rebuilding it.
3. **Stages.** Where a stage's result fits the next stage only at the
   mirrored spelling, the checker records the swap on that stage, and the
   flow lowering wraps the value between the two steps.
4. **Two refusals found beside the commuted-stage fix.** Each gets a test
   first, then a fix or a better diagnostic:
   - `<42 | emit | println` with `fn emit<+T: Display>(out: String) <- T` is
     refused with "`println` needs `Display` for a type parameter": the
     stage read the other way round does not tie `T` to what flows in before
     the bound is discharged. Tie it, and the chain is accepted.
   - `<s | deliver` with `s: -String` and a negative trait method
     `fn deliver(out: String) <- Self` is refused with "no `impl Deliver for
     String`": a consumer flowing in is read as the receiver. Either read it
     as the method's continuation, leaving `Self` to the context, or refuse
     it with a message pointing at the call form `deliver(s)`.
5. **Docs.** `DESIGN.md`'s account of `;` gains structures and stages; this
   file narrows the known limit to type constructors, or removes it.

### 6. Type variables carry a polarity

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

- **Composable capture.** A continuation that returns to where it was
  captured — `shift`'s `k : A -> R` — would be a function rather than a
  consumer, and would bring answer types into the checker, which the
  abortive, handler-delimited `mu` of `DESIGN.md` §6 keeps out — and it is
  what would give `reset` a use beyond refusing jumps. Revisit when a program
  needs one. The typing is worked out in Kobori, Kameyama and Kiselyov,
  "Answer-type modification without tears" (WoC 2015), and Materzok and
  Biernacki, "Subtyping delimited continuations" (ICFP 2011).
