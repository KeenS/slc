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

### 1. A missing `return` clause is the identity, and says so

`handle e { … }` without `return` already gives its body's value
(`crates/slc-syntax/src/lower.rs`, `Expr::Handle`), but `DESIGN.md` never says
so, and the examples write `return(n) => n` in nearly every handler.

1. **Tests first:** a handler without `return` answers its body's value, and a
   test in `crates/slc-check` shows its type is the body's.
2. **Docs.** `DESIGN.md`'s effects section states the default.
3. **Examples.** Drop every identity `return` clause from `examples/`, the
   stdlib and `DESIGN.md`'s programs; each example's output is unchanged
   (compare against a snapshot taken first).

### 2. A misplaced `>` after `resume` is diagnosed

`<v | resume>` cuts `v` into `resume` as if it were a consumer, and the
checker reports "`::0` is an alternative of a sum, and it is used as
(…, dual(?…))", or a mismatch against a tuple, rather than the `>`.

1. **Tests first:** `<::0(x) | resume>` and `<42 | resume>` in a clause are
   refused with a message that names `resume`, says it is a function, and
   shows `<v | resume` without `>`.
2. **The checker.** In the flow arm's closing-consumer branch, when the
   closing stage is a name whose type is a function (a `Par` of a consumer
   and a result, not `(;)`), report that instead of the mismatch. It applies
   to any function closed with `>`, not only `resume`.
3. **Docs.** `MIGRATION.md` needs nothing; `DESIGN.md`'s "Diagnostics"
   section lists the case.

### 3. A chain may stand as a tuple component without parentheses

`<(<p | read_text), "!") | add` is a parse error — "`<` sends a value into a
stage, and this one has none" — and `((<p | read_text), "!")` is required.

Proposed: a chain opened with `<` inside a tuple, a bundle or a call's
arguments ends at the `,` or `)` that closes its component, as it already
ends at `;` and `}`.

1. **Tests first:** `(<p | f, 1)`, `(1, <p | f)` and `(<p | f | g, <q | h)`
   parse to the tuples the parenthesized forms give, and run the same; a
   chain whose stage is itself a tuple, `<x | (f, g)`, keeps its meaning.
2. **The parser.** `crates/slc-syntax/src/parser.rs`: a chain's stage loop
   stops at `,` and `)` when the chain is a component. If a case turns out
   ambiguous, keep the parse and improve the error instead: "wrap this chain
   in parentheses: `(<p | f)`".
3. **Docs.** `DESIGN.md` §3; `MIGRATION.md` notes the parentheses are no
   longer needed; the examples drop them where they only guarded a chain.

### 4. A handler clause may name its operation by path

`fs::read_file(path): resume => …` does not parse, so a handler outside the
module that declares the effect needs `use fs::read_file;` first, while rows
already take paths (`{fs::Fs}`).

1. **Tests first:** a handler for `fs::Fs` written with `fs::read_file(…)`
   clauses and no `use` answers the operation.
2. **The parser.** A clause's operation name is a path, as a row's effect is
   (`parse_effect_row`); resolution already qualifies clause names.
3. **Docs.** `DESIGN.md`'s file-system example and `MIGRATION.md`'s section
   "File operations are an effect" drop the `use`.

### 5. A computation is handed to a handler without a dummy parameter

Installing a handler function reads `<(fn(u: (,)) { … }) | fs::real`, and a
body that ends in a cut cannot be passed at all: `(,) -> (;)` is `(;)`, so
file work that leaves through continuations has to return a status through
`mu i32 { done <= … }` (`examples/file_io.sl`).

Proposed: `fn { … }` is a lambda of no parameters, sugar for `fn(_: (,))`;
and `fs` gains `real_command`, a handler for a program that ends in a cut,
`program: ((;) / {Fs, ..E})`, run under the same clauses.

1. **Tests first:** `<(fn { <"a.txt" | shout }) | with_fake_fs` runs as the
   `fn(u: (,))` form does; a file program ending in `<0 | exit>` runs under
   `fs::real_command` with no status continuation.
2. **The parser and lowering.** `fn {` parses as a lambda whose parameter is
   `_` of type `(,)`.
3. **The stdlib.** `fs::real_command`, sharing its clauses with `fs::real`.
4. **Docs and examples.** `DESIGN.md`'s `IO` section and §9's program,
   `MIGRATION.md`'s file-system section, and `examples/file_io.sl`, which
   loses its `done` continuation.

### 6. A `select` arm may produce a value

A `select` arm must be a command, so a handler clause that routes a
primitive's outcomes back has to capture its own result with
`mu { out <= … <(<v | resume) | out> … }`; `fs::real` repeats that four times.

Proposed: a `select` arm may be an expression, whose value is delivered to
the continuation of the `select`'s activation — `select String { t =>
<::0(t) | resume }` — the way a `match` arm's value is the `match`'s.

1. **Tests first:** a `select` whose arms produce values, activated by a
   cut, hands each arm's value to what follows the cut; a command arm keeps
   its meaning; `fs::real` rewritten without `mu { out <= … }` passes the
   file tests.
2. **The checker.** A `select`'s type records its arms' result: a consumer of
   `A` producing `B` is `(A -> B)`, and an all-command `select` stays
   `-A`. Decide in this step whether that type is the function type itself —
   which makes a value-producing `select` a function — and write the
   decision into `DESIGN.md` §7.
3. **Lowering.** A value-producing arm lowers to its value delivered to the
   activation's continuation.
4. **Docs and stdlib.** `DESIGN.md` §7, `MIGRATION.md`, and `fs.sl`.

### 7. The stdlib's lazy codata carries its rows on the returned type

`seq::map`, `filter` and `take_while` build their rest with `let+` before
storing it in `Step::Yield`, because the payload is declared without a row;
and building a `Seq` performs nothing, yet their signatures charge `/ {..E}`
at the call.

1. **Tests first:** `examples/seq.sl` and the stdlib tests keep their output;
   a `Seq` built by `seq::map` with an effectful function and demanded under
   a handler is accepted, and demanded outside one is refused.
2. **The stdlib.** The functions declare `-> (Seq<B> / {..E})`, the bodies
   store `<(f, rest) | map` directly, and `let+` goes. If `Step::Yield`'s
   payload must carry the row, this step needs "Row variables on
   declarations" (Deferred) first — then stop, and move that entry here.
3. **Docs.** `DESIGN.md`'s stdlib section; the comment in `fs.sl`'s and
   `seq.sl`'s bodies.

### 8. An alternative's sum is read through a consumer function

`<::0(7) | describe | println` with `fn describe(out: String) <- (i64 |
String)` is refused — "`::0` is an alternative of a sum, and it is used as
-String" — because the stage reads the other way round and the sum is not
passed back to the position. `examples/sums.sl` keeps a `mu` for it.

1. **Tests first:** that chain prints `number 7`, and `sums.sl` drops its
   `mu`s; `<42 | classify | …` with a command keeps its meaning.
2. **The checker.** In the flow arm, when the first stage is an injection and
   the next stage is read the other way round, unify the injection's sum
   with the stage's consumed type before the pending injection is resolved.
3. **Docs.** `DESIGN.md`'s sums section, and `examples/sums.sl`.

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

- **Composable capture.** A continuation that returns to where it was
  captured — `shift`'s `k : A -> R` — would be a function rather than a
  consumer, and would bring answer types into the checker, which the
  abortive, handler-delimited `mu` of `DESIGN.md` §6 keeps out — and it is
  what would give `reset` a use beyond refusing jumps. Revisit when a program
  needs one. The typing is worked out in Kobori, Kameyama and Kiselyov,
  "Answer-type modification without tears" (WoC 2015), and Materzok and
  Biernacki, "Subtyping delimited continuations" (ICFP 2011).
