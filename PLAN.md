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

No large feature is mid-flight. The sweep of the known limits and the
ergonomics it turned up have landed; "Next" holds what a review of that
work raised — two soundness gaps and a missing check, small — and then the
three larger features that were waiting on a program to need them, each
with a proposal to confirm first.

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

### A handler clause binds as many parameters as its operation takes

A clause takes parameter *i* from the operation's signature and gives any
extra one a fresh variable (`Expr::Handle`, `crates/slc-check/src/expr.rs`,
"A clause takes what its operation is performed with"); a missing one goes
unnoticed. `fs::write_file(p): resume => …` types `p` as `String` while the
runtime binds the `(path, contents)` tuple (`bind_names` in
`crates/slc-syntax/src/lower.rs`, `Expr::Handle`), and `read_file(a, b)`
aborts at run time with "a consumer of 2 components received …".

Proposed: a clause binds exactly the operation's parameters, and a nullary
operation's clause is written `config()`, never `config(u)` — the runtime's
ignored unit binder is lowering's business, not the program's.

1. **Tests first:** `write_file(p)`, `read_file(a, b)` and `config(u)` are
   refused with "`write_file` takes 2 parameters, and this clause binds 1";
   the clauses in `fs.sl`, `examples/*.sl` and the tests keep their meaning.
2. **The checker.** In `Expr::Handle`, compare `clause.params.len()` with
   `signature.params.len()` before typing them.
3. **Docs.** `DESIGN.md`'s "Diagnostics" list gains the case.

### A returned value's row is the promise's, never the declaration's

`fn mk(exit: -i32) -> -i64 / {Tick}` absorbs what the returned `select`
performs into the call's row (`check_decl`, `Decl::Fn`: "Where that type
carries none, the declaration answers for it"; `docs/design-notes/rows-in-types.md`
§3), so a `handle` around the call discharges nothing and the consumer fires
later, unhandled: `let k = handle (<exit | mk) { tick(): … }; <5 | k>` runs
`tick` with no handler.

Proposed: refuse it. A returned negative value whose promised type carries
no row must perform nothing; the sound spelling is `-> (-i64 / {Tick})`, and
the diagnostic says so. A rowless `menu` or `form` built by a function
carries its arms' row on its value too (`Type::Rowed(menu_ty, runs)` in the
`mu` arm), so nothing needs the old charge; `stream::map`'s `-> Stream<B> /
{..E}` becomes `-> Stream<B, ..E>` with a row parameter on `Stream`, as
`Seq` has.

1. **Tests first:** the program above is refused with "the value `mk` hands
   back performs `Tick`, and its type `-i64` carries no row; write
   `-> (-i64 / {Tick})`"; the same declaration with the row on the type runs
   under the handler; `examples/latent_effects.sl` and the stdlib keep their
   output.
2. **The stdlib.** `menu Stream<+T, E> / {..E}`; `stream::map`, `iterate`,
   `unfold`, `zip`, `drop` declare their rows on the type.
3. **The checker.** Replace `env.perform(actual_row)` in the rowless-promise
   branch with the diagnostic; drop the "declaration answers for it" prose
   from `rows-in-types.md` §3 and `DESIGN.md`'s "Rows are part of types"
   (its second exception).
4. **Docs.** `MIGRATION.md`: the row moves from the arrow to the type.

### A handler names every operation of each effect it handles

The handled set is the effects of the operations the clauses name
(`Expr::Handle`, `let handled = clauses …`), so a mock answering only
`fs::read_file` type-checks a program that calls `fs::write` and aborts with
"no handler for operation fs::write_file" (`split_at_handler`,
`crates/slc-runtime/src/machine.rs`) — against `DESIGN.md`'s "a well-typed
program performs no operation the runtime cannot answer".

Proposed: an effect is handled whole. A handler that names some operations
of an effect names them all, or ends with a forwarding clause `_ => forward`
that re-performs any other operation of that effect to the handler outside
and resumes — which keeps a tap on one operation cheap. Rows keep tracking
effects, not operations.

1. **Tests first:** `handle p { fs::read_file(path): resume => … }` where
   `p` performs `fs::Fs` is refused with "`fs::Fs` has 6 operations, and this
   handler answers `read_file` alone; answer the rest, or add
   `_ => forward`"; with `_ => forward` the program runs and `fs::write` goes
   to the disk under `fs::real_command` outside; a handler naming every
   operation needs no clause.
2. **The parser.** A last clause `_ => forward` on a handle.
3. **The checker.** Per effect named by a clause, the missing operations are
   an error unless the handler forwards; a forwarding handler's body row
   keeps that effect (it is not discharged), so the outer handler is
   required by the row.
4. **Lowering and the runtime.** A forwarding handler's prompt carries no
   entry for the unnamed operations, so `split_at_handler` walks past it to
   the outer handler as it does today; the resumption crossing it carries a
   copy, as any prompt does. So the runtime needs nothing; lowering ignores
   the `forward` clause after the checker has used it.
5. **Docs.** `DESIGN.md`'s effects section and the "Diagnostics" list;
   `docs/design-notes/file-system-effect.md` "Consequences".

### A command closed on functions yields their result

A chain closes on a command's exits, `<path | __read_file | (k1 & k2)>`, and
the exits are consumers, so an outcome can only leave through a continuation
captured for it: `fs::real` and `fs::real_command` write `mu { out <= …
<(<v | resume) | out> … }` four times each. A value-producing `select` would
not help — `select T { p => v }` is `fn(x: T) { match x { p => v } }` in type
and meaning, and the bundle would still have to close the chain.

Proposed: a command's menu of exits may be met by a bundle of functions
instead of consumers. Where the command offers `(-A & -B)` and the closing
stage has type `((A -> R) & (B -> R))`, the chain does not end there: its
value is the `R` the chosen function produces, and it composes on —
`<path | __read_file | (fn(t) { ::0(t) } & fn(w) { ::1(w) }) | resume`. It
is sugar for `mu { out <= <… | (f₁ | out> & f₂ | out>)> }`, and `select`
keeps meaning one thing.

1. **Tests first:** the chain above prints what the `mu` form prints; a
   bundle mixing a function and a consumer is refused with "exit 2 is a
   consumer, and exit 1 a function producing `R`; a command yields a value
   only when every exit does"; the result type is the functions' common
   `R`, and the chain composes on through `| println`; `fs::real` and
   `fs::real_command` rewritten without `mu { out <= … }` pass the file
   tests.
2. **The checker.** The command signature arm in `Expr::Flow`
   (`crates/slc-check/src/expr.rs`): when the closing stage's items all have
   type `(Aᵢ -> R)` against declared exits `-Aᵢ`, the stage is not the last
   and `acc` becomes `R`; `FlowShape` records the command's index as a
   `yielding` stage.
3. **Lowering.** `Expr::Flow` in `crates/slc-syntax/src/lower.rs`: a
   yielding command lowers to `μout. ⟨command values (f₁·out & … & fₙ·out)⟩`,
   each `fᵢ·out` the function composed into `out`, and the rest of the chain
   applies to the `mu`'s value.
4. **Docs and stdlib.** `DESIGN.md` §5 (`command`) gains the rule and
   `MIGRATION.md` shows the `mu` form beside it; `fs.sl` loses its `mu {
   out <= … }`.

### Handler values

A handler is an ordinary function today, taking the computation it handles
(`fs::real`, `fs::real_command`). A program that stores a handler, chooses
one at run time, or composes two needs the handler as data.

Proposed, to confirm before any step: `handler [Effect] { clauses }` is an
expression of type `Handler<A, B, {E}, {F}>` — it turns a computation
producing `A` under `{E, ..F}` into one producing `B` under `{F, ..}` (the
`return` clause maps `A` to `B`; without one, `A` is `B`); `with h handle c`
installs it, and is what `handle c { … }` desugars to. Lowering already
builds the clause tree as a value (`Expr::Handle` in `lower.rs`, the
`__clauses` tag), so the runtime has the representation; what is new is the
type and the two syntaxes.

1. **Tests first:** `let h = handler Reader { config(): resume => <10 |
   resume }; with h handle (<7 | scaled)` prints `70`; a handler stored in a
   list and chosen by index; `with h handle c` where `h`'s effect is not in
   `c`'s row is refused; composing two handlers by nesting `with`.
2. **The parser.** `handler`, `with … handle …`; `handle c { … }` parses as
   before and desugars in the checker or lowering.
3. **The checker.** The type, its four arguments, and `with`: the body's row
   minus `{E}` fits the outer row; the clauses check as today.
4. **Lowering.** `handler { … }` is the clause tree; `with h handle c` is
   `__handle(h, thunk c)`. The runtime needs nothing.
5. **Docs.** `DESIGN.md`'s effects section; `docs/design-notes/file-system-effect.md`
   "Proposal" item 1 is superseded and says so.

### Composable capture

A continuation that returns to where it was captured — `shift`'s `k : A ->
R` — is a function, not a consumer, and brings answer types into the
checker, which the abortive, handler-delimited `mu` of `DESIGN.md` §6 keeps
out. It is what would give `reset` a use beyond refusing jumps.

Proposed, to confirm before any step: no new primitive. A resumption is
already a function value (`Value::Resume`, composed onto the running stack
by `append`, `crates/slc-runtime/src/machine.rs`), so `shift` is a
one-operation effect handled by `reset`: `effect Shift<+A, +R> { fn shift(f:
((A -> R) -> R)) -> A; }` in the stdlib, with `reset` handling it by `shift(f):
resume => <resume | f`. The answer type `R` is the handler's body type, so
the checker's existing handle typing carries it and no answer-type
modification is needed; what is refused is a `shift` whose `R` differs from
the enclosing `reset`'s, which the row's type arguments express.

1. **Tests first:** `reset (<(fn(k) { <(<1 | k), (<2 | k)) | add }) | shift |
   x => (x, 10) | mul)` prints `30` (two returns through one continuation);
   a `shift` outside any `reset` is refused by the row; the answer type is
   the `reset`'s.
2. **The stdlib.** `control::Shift`, `control::reset`.
3. **Docs.** `DESIGN.md` §6 gains a paragraph, and "No shifts" is checked
   against it — the polarity shifts it refuses are unrelated.

## Deferred, for discussion

Nothing. Each open question above is an entry marked "Proposed", confirmed
before its change.
