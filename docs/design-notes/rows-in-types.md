# Rows in types

Status: reviewed; being implemented — `PLAN.md`, "Rows live in types". The
decisions move to `DESIGN.md` as they land, and this note records why they
were taken.

## Why

Effect rows are checked today by a pass that follows names
(`crates/slc-check/src/effects.rs`), beside the type checker rather than
inside it. The type checker lowers every written row away
(`TypeExpr::Effectful(inner, _)` resolves to `inner` in `expr.rs`,
`declarations.rs` and `lower.rs`), and the effect pass reconstructs what it can
from declarations and names. What names cannot follow is the known limit
"Effect tracking follows names":

- a lambda's effects are charged where it is written, not where it runs;
- a higher-order global passed as a value forwards its concrete row but not
  its row variables;
- a function laundered through a `let` is not tracked;
- a stage's row variables are instantiated only for the first stage of a
  chain;
- a delayed computation stored in a tuple, bundle, variant or record that
  performs anything is refused, and so is a delayed row with a variable
  (`DESIGN.md` §4, "When a `let` computes").

Each is a value whose behaviour when run is not visible from its name. A type
travels with the value, so a row on the type follows it into a tuple, through a
`let`, and out of a call.

## What does not change

The surface. Rows are written where they are today: after an arrow,
`(A -> B / {Exn, ..E})`; on a returned consumer, `-> (-A / {..E})`; on a
declaration's row, `fn f(…) -> T / {IO}`; on a menu or form declaration,
`menu Fallible / {Exn} { … }`. Row variables are generic parameters used with
`..`. `main`'s row is `{IO}` or empty. A `handle` discharges the effects of the
operations its clauses answer. The diagnostics keep their messages.

## 1. Representation

The core has no arrow: `A -> B` is `(dual(A) ; B)`
(`slc_core::types::Type::arrow`). A row describes what *running* a value
performs, and what runs is a negative value: a function (a `Par`), a consumer
(`Neg`, or `Dual` of a variable), a menu or form (`Named`). So the row cannot
live on `Par` alone: `-A / {Exn}` is an atom, and `Fallible` is a name.

**Proposed:** a wrapper.

```rust
enum Type {
    // …
    /// A negative type together with what running a value of it performs.
    Rowed(Box<Type>, Row),
}

struct Row {
    /// Concrete effects, by name.
    effects: BTreeSet<String>,
    /// At most one row variable: flexible (an inference variable) or rigid (a
    /// declaration's own `E`, seen from inside its body).
    tail: Option<RowVar>,
}
```

- **An unrowed negative type is the empty, closed row.** `Rowed(t, {})` is
  normalized to `t`, so every existing type is already a rowed type and
  nothing that does not mention rows changes shape.
- **`dual` maps through the wrapper,** `dual(Rowed(t, r)) = Rowed(dual(t), r)`,
  so `dual` stays an involution. A positive `Rowed` arises only as the dual of
  a negative one — the argument side of a commuted `;` — and means nothing of
  its own; nothing performs it.
- **`;` stays commutative.** The row wraps the whole `Par`, so turning its
  components around (`commute`, `swap_adapter`) keeps it. `commute` and the
  flow arm look through `Rowed` to the connective beneath.
- **`is_positive` and `is_negative`** answer for the type beneath.
- **Display** is the surface spelling: `(i64 -> i64 / {Exn})`,
  `(-String / {Exn, ..E})`, `Fallible / {Exn}`.

The alternative, a `Row` field on `Par` and `With`, leaves consumers and names
without a place for a row, and every match on those variants would change.

## 2. Subeffecting: inclusion, not unification

A pure function may stand where one that performs `Exn` is allowed; today that
is the rule "a rowless parameter is a promise of purity", read the other way.
Two ways to get it:

- **Rémy-style row unification.** Rows are unified as equal, with tails
  absorbing the difference. A pure function against `/ {Exn}` then fails unless
  every row in a signature is opened with a fresh tail when it is instantiated
  (Koka's approach), which makes every written row polymorphic and every error
  message mention a variable.
- **Inclusion constraints.** Where a value meets a slot, its row must be
  included in the slot's: `actual ⊆ expected`. Constraints are collected while
  checking a declaration and solved when it is done, beside the checker's other
  deferred work (`pending_lets`, `pending_dicts`).

**Proposed:** inclusion. It is the rule the effect pass already enforces, the
checker already has a per-declaration solve phase to put it in, and a
diagnostic can name the effect that does not fit.

**Solving.** Each flexible row variable gets the least row satisfying its lower
bounds: propagate `r₁ ⊆ ρ` along the constraints until nothing changes, then
check every constraint whose right side is concrete or rigid. A rigid tail `E`
is an atom: it fits only where the upper bound names `..E`. A failing
constraint reports the first effect, or `..E`, outside its bound, at the span
that recorded it.

**Where inclusion applies.** Unification is symmetric and has no notion of
variance, so direction is taken only where the checker knows which side is the
value and which the slot: `fits(expected, actual)`, an argument meeting its
parameter, a value cut into a consumer, a result meeting its declared type.
There the top-level rows get `actual ⊆ expected`.

**Nested rows,** inside a component of a tuple or an argument of a named type,
are unified as equal: `List<(A -> B / {Exn})>` and `List<(A -> B)>` do not
meet. That is sound and stricter than needed; a variance-aware version, which
would flip the direction under each `dual`, can follow if a program needs it.

## 3. Where effects happen

Every body is checked against a *current row*: a flexible row variable `ε`
that stands for what running the body performs. Each point that performs adds
its row to `ε`:

| where | adds |
|---|---|
| performing an operation of effect `E` | `{E} ⊆ ε` |
| calling a function of type `T / r` — a call, or a stage | `r ⊆ ε` |
| cutting a value into a consumer of type `-A / r` | `r ⊆ ε` |
| demanding an item of a menu of type `M / r`, or of a rowed menu declaration | `r ⊆ ε` |
| feeding a form of type `F / r`, or a rowed form declaration | `r ⊆ ε` |
| running a delayed computation of type `T / r` | `r ⊆ ε` |
| a file primitive beneath `fs` (`builtin_effect`) | `{IO} ⊆ ε` |

Bodies introduce their own current row:

- **A declaration.** `ε_decl ⊆ written row`, and for `main`, `ε_main ⊆ {IO}`.
  Its declared type carries the written row on its arrow, so a call adds it.
- **A lambda.** Its body has a fresh `ε_λ`, and its type is
  `(A -> B / ε_λ)`: the effects happen where the lambda is called. This closes
  "a lambda's effects are charged where it is written".
- **A `handle`.** Its body has a fresh `ε_body`, with
  `ε_body ⊆ ε ∪ {the effects its clauses answer}`. The clauses run below the
  prompt and add to `ε` directly. The handle's value has the body's type, row
  included: a row on a value has not fired, so it passes through.
- **A `select` literal.** Each arm's row is the row of the consumer's type,
  `-A / ε_arm`. Over a rowed form declaration, `ε_arm ⊆ declared row`, as today.
- **A menu literal `mu M { … }`.** Each arm's row goes onto `M`'s type; over a
  rowed menu declaration, `ε_arm ⊆ declared row`, as today.
- **A delayed computation.** What it performs where it runs joins its type's
  row: `let- f = e`, with `e : T / r` and `e` itself performing `c`, binds
  `f : T / (c ∪ r)`. Stored in a tuple, the row stays on the component's type,
  so the refusals in "Why" are lifted.

**Row variables.** A declaration's generic `E` is a template: instantiated at
each call as a fresh flexible row variable, which the arguments' rows bound
from below through their parameters; inside the declaration's own body it is
rigid. So `map(half, xs)` gives `{Exn} ⊆ ρ` from `half` meeting
`f : (A -> B / {..ρ})`, and the call adds `ρ ⊆ ε`. A stage is a call, so every
stage instantiates its callee's row variables, not only the first.

**Generalization.** A generalized `let` (a value, under the value restriction)
generalizes the row variables of its type together with its type variables.
Rows on a declaration's signature are written, so nothing is inferred there.

## 4. Consequences

- **Newly accepted:** a stored delayed computation that performs; a delayed
  row with a variable; a lambda written inside a handler and called outside
  it, when the call is under a handler of its own; a function laundered
  through a `let` and called under a handler.
- **Newly refused:** a lambda written under a handler and called outside any —
  the handler around the writing never answered it, and the run failed with
  "no handler for operation"; a function laundered through a `let` whose row
  its caller never declared. Both were run-time failures or silent gaps.
- **The stdlib's lazy codata.** `seq::map` performs nothing when called — it
  builds a menu — but its signature says `/ {..E}`, because a row could not ride
  on the returned `Seq`. With rows on types it can be written
  `-> (Seq<B> / {..E})`, and the `let+` its body now needs goes away. That is
  an optional follow-up, not part of this change.

## 5. Staging

1. **The type.** `Type::Rowed` and `Row` in `slc-core`, with `apply`, `occurs`,
   `dual`, `instantiate`, `freshen`, display and the parser. Rows are all
   empty, so nothing changes; the existing suite is the test.
2. **Rows in signatures.** `signature_type` and `resolve_with_self` keep
   written rows. The checker records inclusion constraints and solves them per
   declaration, but only reports them in tests, which compare its verdict with
   `effects.rs` over every program in the suite.
3. **The switch.** The solver's diagnostics replace the effect pass's; the
   refusals of stored and row-variable delayed computations go; `effects.rs` is
   deleted, or keeps only what types cannot say. The tests for each gap in
   "Why" land here.

## Decided

Reviewed, with every proposal taken:

1. Rows fit by inclusion constraints, solved per declaration.
2. Nested rows are unified as equal in the first version.
3. Menu and form declarations keep concrete rows; row variables on
   declarations are later work.
4. The stdlib's lazy codata moves its rows to the returned type after this
   change, not in it.
