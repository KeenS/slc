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
remain. What `Next` and `Deferred` hold now is what writing programs in the
language turned up — the formatter, the editor mode, and the examples under
`examples/programs/` — rather than anything the redesign left undone.

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

- **Programs of more than one file.** `slc run` takes one source, and the
  only other units are the prelude and `stdlib/`, compiled into the driver.
  `mod` names a scope but cannot name a file, so a program grows only
  downward: `examples/programs/json_parser.sl` is 350 lines for want of
  anywhere else to put them. A module should be loadable from a file beside
  the program, as a source unit like the library's — resolution already
  scopes imports by unit. Settle the mapping from `mod name` to a path, and
  what a diagnostic calls a span in such a unit, in a design note first.

## Deferred, for discussion

Each of these needs a decision before it is work. Once one is made it goes
to `DESIGN.md`, and whatever it leaves to build moves up to `Next`.

- **Floats: finish them or refuse them.** A float literal lexes, parses, and
  matches as a pattern, but there is no float base type, and the checker
  gives the literal the type of unit — so `let x = 1.5;` is accepted and
  means nothing. Either floats become a base type with their arithmetic and
  `Display`, or the literal is refused the way `if` is, with a message that
  says so. The half-state is the one wrong answer.

- **Building strings.** Text is assembled a pair at a time —
  `<("a", b) | add | x => (x, "c") | add` — and it is the most repeated shape
  in the examples; the prelude's `Display` for tuples is eight copies of it.
  The `format` builtin only joins its arguments with spaces and nothing uses
  it. The question is what the surface should offer instead: an
  interpolating literal, a variadic `concat`, or a `Display`-driven builder
  — and whether that is syntax or only library.

- **Collections beyond `List`.** The one container is a linked list, and the
  one indexed thing is a `String`, so every lookup is linear and there is no
  map, set, or array. What is open is where they belong — builtin types
  with builtin operations, as `String` is, or library types over some
  smaller primitive — and what an indexed structure means on the negative
  side, where `Stream` and `Seq` already mirror `List`.

- **What traits still lack.** A trait is a set of signatures: no default
  methods, no associated types, no supertraits. Each is ordinary in Rust,
  whose flavour the surface keeps; whether each earns its place here, given
  that dispatch is resolved entirely at compile time, is undecided.

- **Integer operations.** The four widths have arithmetic and comparison
  and nothing else: no bitwise or shift operations, and no conversion from
  one width to another, so a value cannot move between `i32` and `i64`.
  `stdlib/num.sl` offers `min`, `max` and `abs`, over `i64` alone. Decide
  the set, and whether conversions are functions or a trait.
