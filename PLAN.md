# Slant Redesign Plan: λ̄μμ̃ and Additive/Multiplicative Duals

## Goal

Realign Slant with the classical lambda-bar-mu-mu-tilde calculus (λ̄μμ̃),
preserve a Rust-like surface, and make the syntax symmetric without pretending
that a Rust-like surface can itself be literally symmetric.

## Status

The redesign is finished: every phase in the record below is complete, and the
acceptance suite is green.

`DESIGN.md` is the language reference. This file records what the redesign
settled and what it deliberately left undone; it does not define the language,
and where the two disagree, `DESIGN.md` is right.

## What the redesign settled

### Core

- The core gained the constructs the surface needs: `Λα. t` (a declared
  continuation parameter, distinct from `μα. c`), `L(t)` (a labelled additive
  injection — an enum value or a struct), and `μ̃[…]` with `co(e)` (a consumer
  and its reification as a value).
- A binder lowers to `μ̃`, the value abstraction, and `λ̄` is application and
  nothing else — so the evaluator no longer tells them apart by the binder's
  name. `let x = v; body` is `μlet. ⟨v ∥ μ̃x. ⟨body ∥ let⟩⟩`, and the μ̃ rule
  applies to any value rather than only to a λ.
- `A → B` is `-A ⅋ B`, so `Type::Fun` is gone: a function is negative,
  `dual(A → B)` is the call stack `A ⊗ -B`, and a cut is checked by duality
  rather than by which side is written negatively. A negative function's
  declared type dualizes its result, not the whole function.
- `A → ⊥` and `-A` are one type: a lambda whose body ends in a cut *is* a
  consumer, and a continuation may be annotated either way.
- `dual(A)` applies the involution rather than wrapping a node, and
  `Type::Dual` carries a polarity, so `dual(A)` is usable wherever the
  corresponding sign is.
- A cut lowers to `Command::Cut` for a named consumer and `Command::Activate`
  for a computed one, so the evaluator no longer decides what `x(y)` means by
  inspecting the runtime value.
- The core's printed form is exact: `slc_core::parse` reads back what
  `Display` writes.

### Surface

- `spawn` is gone and the `from`/`to` parameter markers are removed. A
  declaration that takes values and continuations is a `command`, whose
  parameter groups are written only when they have parameters; `mu` names the
  expression that captures the current continuation, and nothing else. One
  keyword for both hid that they are different core constructs — `Λα. t`, a
  parameter the caller supplies, against `μα. c`, the ambient continuation.
- Every `fn` declares its direction with an arrow: `->` for a positive
  function, `<-` for a negative one. Either may take continuation parameters.
- Application and cut are distinct: `f(a)` applies at either polarity, and
  `v @ k` is the cut — a command of type `⊥` that does not return. Calling a
  continuation is rejected, and `select` arms are cuts.
- `match` takes any positive value apart, and `select` builds the consumer of
  any positive type, with arms `pattern <= command`: one arm per variant of an
  `enum` (the negative additive), exactly one for a `struct` or tuple, binding
  every component (the negative multiplicative), and one for an atom — the
  degenerate product — whose plain binder takes the whole value, which is the
  surface spelling of `μ̃x. c`.
- `⅋` therefore has a construction syntax, and it is always supplied whole:
  feeding halves independently needs a returning send or concurrency, and the
  language has neither.
- `enum` variants carry payloads, which pack into one tensor; `Color::Variant`
  is an enum-value expression; a struct literal is an ordinary expression and
  a struct pattern binds its fields, a struct value being the labelled product
  an `enum` variant already was.
- A `mu` expression is `call/cc`: it abstracts over the continuation it is cut
  against and over nothing else — a value parameter would make it a lambda,
  which is what `fn` is. Its type is what the captured continuation receives,
  so a call whose result arrives through a continuation no longer nests the
  rest of the program inside it.
- An integer literal takes the integer type its port requires, `()` is the
  unit value, and a call with no arguments applies its callee to a marker that
  carries none — so `f()` and `f(())` are no longer the same thing.
- The entry point is `command main | (exit: -i32)`: a program is a command, its
  status is the value it cuts against `exit`, and linearity makes every
  terminating path leave through it. There is no final-result value; output is
  what the program prints. There is no global `EXIT` either — ending the
  program is a right a helper is handed, as a continuation, never one it
  takes.
- The standard library follows the declaration rule: `parse_int`, `read_file`,
  `write_file`, `char_at`, `list_get`, `map_get`, and `find_char` take a
  continuation per outcome and activate exactly one. Operator failures (`s[i]`,
  division by zero) stay fatal.
- The shifts `↓`/`↑` box a negative type as data and dualize through each
  other without cancelling, and the same glyphs are the (erased) expression
  coercions. Data positions, the left of `@`, and `select`'s type are
  positive, so a consumer travels only in a box — which is what makes `¬¬A`
  the distinct type `↓↑A` and rejects `dne(42)`.
- Outcomes are a continuation row, not an `enum` sent to one continuation: a
  consumer of `A ⊕ B` is a consumer of `A` together with a consumer of `B`, so
  the enum wraps what the row already says — and the row additionally says
  which outcomes each operation has. An `enum` stays the way to write data a
  program holds.

### Checking

- Linearity checks that a continuation is not dropped, not how many times it is
  mentioned: a cut does not return, so at most one mention can run. Values stay
  linear in both directions. A positive function returns, so consumers it
  receives are shared rather than linear.
- Polarity checking resolves declaration names, so a parameter typed by a
  `struct` or an `enum` is no longer skipped.
- A `match` on a product is exhaustive with one arm: an unguarded irrefutable
  pattern covers the type, so a single-shape type needs no `_`.
- Surface AST spans are source byte offsets, so a diagnostic quotes the text it
  is about.
- The expression checker runs on the core's unifier: an unannotated binder,
  a closure's parameter, a generic instantiated at its call, a builtin's
  element type are variables solved by use, and nothing fits everything —
  `1` is the unit type and only that. A body is checked against what its
  declaration promises, a call's arguments against what it declares, and a
  type parameter is rigid inside its own body.
- A type is written where nothing else says it. A declaration's parameters
  always carry one; inside a declaration, a lambda's parameter and result, a
  local `mu`'s name and parameter types, and a `select`'s type may be left out
  — the local `mu`'s from the slot its body passes it to or the value it is
  cut against, the `select`'s from an arm that names it or from the enclosing
  negative `fn`. What nothing says is still written, and saying so is a
  diagnostic rather than an unchecked hole.

## Known limits

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, type variables carry no polarity kind, and the untyped evaluator
  remains the backstop for whatever that gap hides.

- **Captured continuations escape rather than resume.** The evaluator unwinds
  to the `mu` that captured a continuation, so one used after its `mu` has
  answered fails, saying so. Lifting this needs the evaluator to hold the
  context as data — an abstract machine with an explicit, re-instatable
  continuation stack — rather than as Rust stack frames.
  `examples/classical.sl` stays inside the limit deliberately.

## Deferred, with no accepted replacement

- **Negative partial application.** The partial-agent forms
  (`agent.consume(k, h)`, `fn.partial(a)`) are removed and nothing replaces
  them; a future design needs its own lowering and tests.
- **`choose T { Variant }`.** Removed along with its lowering, checking, and
  tests while its design is deferred.
- **The internal name `Command`.** The core types and evaluator still use it as
  a Rust type name. That is internal naming, not surface syntax, and is
  acceptable unless renamed separately.
- **`Result` and `Option` in the prelude.** Removed: error handling is
  continuation-based, so neither is canonical any more.

## Record of the redesign

Every phase is complete; each was validated with `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and the
example suite.

- **Phase 0** — settled the working tree: removed `spawn`, renamed `command`
  to `mu` (and later back, once `mu` was needed for the capturing expression
  alone), removed the old value-returning expression `mu`, allowed
  continuation parameters in `fn`, and added `EXIT: -i32`.
- **Phase 1** — parameter syntax: removed `from`/`to`, introduced
  `mu(values) | (continuations)` with the optional `-> ⊥` annotation, and added
  migration diagnostics. A group with no parameters is since left out rather
  than written `()`.
- **Phase 2** — polarized functions by type rather than by keyword: `->` and
  `<-`, rejecting bare `fn` and the `+fn`/`-fn` prefixes, with continuation-row
  equality and inference.
- **Phase 3** — `select`: from a type-declaring form to the consumer of an
  existing positive type, with exhaustiveness, alternative-aware linearity, and
  a genuine core representation rather than a builtin marker.
- **Phase 4** — continuation-based error handling: the JSON parser rewritten
  without `Result`, with the semantics of `e?` and `e?k` defined and tested.
- **Phase 5** — documentation: `DESIGN.md` rewritten as a standalone reference
  with the final grammar, the lowering table, the diagnostic categories, and a
  migration guide, each row of the lowering table tied to a fixture test.

## Acceptance

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test --workspace` pass.
- The example suite runs every example with expected output and exit status.
  `linearity_error.sl` and `polarity_error.sl` are the two intentional
  failures; every other example succeeds.
- `examples/connectives.sl` writes all four connectives in both polarities.
- Diagnostics carry source locations, and driver tests pin the entry-point
  rules: missing `main`, malformed `main`, exit codes, and output ordering.

## Next

- **`open_file` and `close_file`.** Builtin commands over a file handle, so
  reading stops being whole-file-or-nothing: `open_file(path)` offers the
  handle to one continuation and a message to the other; `read_line(handle)`
  or similar reads through it; `close_file(handle)` consumes it. A handle is
  a positive value that should be closed on every terminating path — the
  first resource whose lifetime the linearity checker can watch. Rewrite
  `examples/file_io.sl` around them.
- **Rust-style namespaces.** `mod` blocks, `use`, and `::` paths beyond enum
  variants — `Color::Red` is already path-shaped. A module system decides
  what a bare name means, so it touches every lookup the checker keeps
  (declarations, functions, constants), the lowering's name scheme, and how
  builtins are addressed (`std::io::read_file` against today's flat names).
