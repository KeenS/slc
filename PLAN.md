# Slant Redesign Plan: λ̄μμ̃ and Additive/Multiplicative Duals

## Goal

Realign Slant with the classical lambda-bar-mu-mu-tilde calculus (λ̄μμ̃), preserve a Rust-like surface, and make the syntax symmetric without pretending that a Rust-like surface can itself be literally symmetric. The implementation history and the next redesign step are both tracked here; checkboxes reflect genuinely completed work only.

Current implementation status:

- The surface keyword and AST declaration for `spawn` are removed.
- The surface declaration formerly called `command` is now named `mu`.
- The old expression-level/value-returning `mu` syntax is removed.
- Ordinary `fn` can currently declare continuation parameters.
- `EXIT: -i32` is available as the top-level continuation.
- `select` is complete: it consumes an existing enum, lowers to a genuine negative additive consumer (`co(μ̃[…])`) in the core, binds variant payloads, and activates exactly one arm at run time.
- The core has three constructs the earlier plan lacked: `Λα. t` (a declared continuation parameter, distinct from `μα. c`), `L(t)` (a labelled additive injection — an enum value), and `μ̃[…]` with `co(e)` (the negative additive consumer and its reification as a value).
- `enum` variants carry payloads; several payload values pack into one tensor.
- The core's printed form is exact: `slc_core::parse` reads back what `Display` writes.
- Surface AST spans are source byte offsets, so a checker diagnostic quotes the text it is about.
- Application and cut are distinct surface constructs: `f(a)` applies at either polarity, and `v @ k` is the cut — a command of type `⊥` that does not return. Calling a continuation is rejected, `EXIT` is a consumer (`0 @ EXIT`), and `select` arms are cuts.
- A cut lowers to `Command::Cut` for a named consumer and `Command::Activate` for a computed one, so the evaluator no longer decides what `x(y)` means by inspecting the runtime value.
- Linearity applies to declarations whose body is a command: a positive function returns, so consumers it receives are shared rather than linear.
- An integer literal takes the integer type its port requires; `()` is the unit value.
- A declaration that takes both values and continuations is a `mu`: the JSON example is written that way throughout, with each parser ending in a cut instead of returning a position.
- The standard library follows the same rule: `parse_int`, `read_file`, `write_file`, `char_at`, `list_get`, `map_get`, and `find_char` take a continuation per outcome and activate exactly one. Operator failures (`s[i]`, division by zero) stay fatal.
- Linearity checks that a continuation is not dropped, not how many times it is mentioned: a cut does not return, so at most one mention can run. Values stay linear in both directions.
- `select` builds the consumer of any positive type, with arms `command => pattern`: one arm per variant of an `enum` (the negative additive) and exactly one for a `struct` or tuple, binding every component (the negative multiplicative). `⅋` therefore has a construction syntax; it is always supplied whole, because feeding halves independently needs a returning send or concurrency.
- A struct literal is an ordinary expression and a struct pattern binds its fields; a struct value is a labelled product, the shape an `enum` variant already had.
- The entry point is `mu main() | (exit: -i32)`: a program is a command, its status is the value it cuts against `exit`, and linearity makes every terminating path leave through it. There is no final-result value; output is what the program prints.
- A call with no arguments applies its callee to a marker that carries none, so `f()` and `f(())` are no longer the same thing.
- `dual(A)` applies the involution rather than wrapping a node, and `Type::Dual` carries a polarity (the dual of a positive type is negative), so `dual(A)` is usable wherever the corresponding sign is.
- Polarity checking resolves declaration names, so a parameter typed by a `struct` or an `enum` is no longer skipped.
- A `match` on a product is exhaustive with one arm: an unguarded irrefutable pattern covers the type, so a single-shape type needs no `_`.
- `examples/connectives.sl` writes all four connectives in both polarities, with the units; it replaces the separate tensor and par examples.
- A local `mu` used as an expression is `call/cc`, and its type is what the captured continuation receives, so a call whose result arrives through a continuation no longer nests the rest of the program inside it.
- `A → ⊥` and `-A` are one type: a lambda whose body ends in a cut *is* a consumer, and a continuation may be annotated either way.
- The two intentionally failing examples are `linearity_error.sl` and `polarity_error.sl`; every other example runs successfully.
- `to` is removed from the lexer and parser. `from` is not a lexer keyword.
- The core Rust types and evaluator still use `Command` as a type name. This is internal naming, not surface syntax, and is acceptable unless renamed separately.
- `DESIGN.md` has been rewritten around the current syntax; migration references are historical and are not supported surface syntax.
- The latest syntax decisions supersede all earlier `select` and function-polarity designs in this plan.
- `fn` declarations now require an arrow: `->` for positive functions and `<-` for negative functions.
- `Color::Variant` now parses as an enum-value expression and lowers to a tagged enum value.

---

## 1. Final Surface Model

### Positive function

A function whose declaration uses `->` is the orthodox value-to-value function:

```sl
fn add(x: +i32, y: +i32) -> i32 {
    x + y
}
```

### Negative function

A negative function uses the reverse arrow `<-`. Its parameters are continuations, and the type after the arrow is the continuation type that it produces:

```sl
enum Status { Ok(i64), Failed(i64) }

fn report(success: -i64, failure: -i64) <- Status {
    select Status {
        success(code) => Ok(code),
        failure(code) => Failed(code),
    }
}
```

Generic type application in a declaration name (`Result<i32, i32>`) is not
accepted; a negative function names an existing declaration.

The result should read as a continuation, not as a value-returning function. The arrow identifies the direction of the cut:

- `fn(value_params) -> Output` consumes values and produces a value.
- `fn(continuation_params) <- ContinuationType` consumes continuations and produces the continuation on the right of the reverse arrow.

For the negative form, each continuation parameter is consumed, and the continuation type on the right of `<-` is produced.

### Consumer abstraction

`mu` remains the cut-oriented control declaration. Value parameters and continuation parameters are now written in separate parenthesized groups, and `from`/`to` markers are removed:

```sl
mu route(x: +i32) | (k: -i32) {
    k(x)
}
```

The declaration always returns a command. An explicit `-> ⊥` may be allowed as documentation, but it does not change the meaning.

### Negative additive construction

`select` no longer defines a new type. It constructs a continuation from an existing enum:

```sl
enum Color { Red, Green, Blue }

fn k(return: -i32) <- Color {
    select Color {
        return(0) => Red,
        return(1) => Green,
        return(2) => Blue,
    }
}
```

The expression returns a continuation. Activating it with an enum value dispatches to the matching arm. In this form, the negative function receives the consumer continuation `return`; the selected arm supplies the enum value that cuts against it.

### Top-level exit

```sl
EXIT(0)
```

`EXIT` remains a builtin continuation of type `-i32`.

---

## 2. Remove `from` and `to`

### Decision

`from` and `to` are directional English words and make the grammar less symmetric. Parameter polarity is already expressed by `+`/`-` and by the separate parameter groups of `mu`.

Both markers are removed from the lexer and parser. Old `from`/`to` parameter syntax is rejected.

### New grammar

- `fn(params) -> ReturnType { body }`
- `fn(continuation_params) <- ContinuationType { body }`
- `mu(value_params) | (continuation_params) { body }`
- `mu(value_params) | (continuation_params) -> ⊥ { body }` (optional annotation)

### Checklist

- [x] Remove `from` and `to` keywords from lexer and parser
- [x] Update all parser tests
- [x] Update all checker diagnostics
- [x] Update all examples
- [x] Update `DESIGN.md`
- [x] Reject old `from`/`to` syntax with a clear migration diagnostic

---

## 3. Distinguish Function Polarity by Types

### Decision

Do not split `fn` into `+fn` and `-fn`. Use the same `fn` keyword for both forms and distinguish polarity by the arrow and parameter/return types:

| Form | Positive/negative meaning | Core idea |
|---|---|---|
| `fn(P) -> R` | positive function | value-to-value lambda/producer |
| `fn(K₁, …, Kₙ) <- K` | negative function | consumes continuation row `K₁, …, Kₙ`; produces continuation `K` |

### Design questions to resolve while implementing

- [x] Define the continuation parameter row as a comma-separated parameter list whose types must be negative
- [x] Require an arrow on every `fn` declaration
- [x] Reject `+fn` and `-fn` prefixes with a migration diagnostic
- [x] Reject positive parameters in a negative function
- [x] Decide that negative-function partial application is deferred rather than represented by a partial-agent form
- [x] Remove `agent.consume(k, h)` (`Service`) and `fn.partial(a)` (`Job`) partial-agent forms
- [x] Document that partial-agent forms are removed and have no accepted replacement
- [x] Add parser/runtime rejection tests proving the old partial-agent surface forms are not accepted
- [x] Add an implementation reminder to define and test a future accepted negative partial-application form if one is designed
- [x] Decide that a negative function is always an ordinary negative abstraction; local `mu` is the only explicit current-continuation capture form
- [x] Add tests proving that negative function declarations and local `mu` capture do not conflict
- [x] Implement boolean `and`/`or` lowering instead of emitting the `__unimplemented_boolean_operator` marker
- [x] Add short-circuit, linearity, and runtime tests for boolean `and`/`or`
- [x] Define equality/subtyping rules for continuation rows
- [x] Add checker tests for continuation-row equality, ordering, and rejection of incompatible rows
- [x] Add inference tests for empty, singleton, and multi-continuation negative-function rows
- [x] Add inference tests proving that negative-function output types are dual to the declared continuation type, not accidentally collapsed by polarity unification
- [x] Define the accepted status of generic function type parameters and whether they are supported for negative functions and continuation rows
- [x] Add lowering and inference tests for generic positive and negative functions, or reject unsupported forms with a precise diagnostic
- [x] Make negative-function lowering explicit in the core calculus rather than relying only on inference-time duality
- [x] Ensure negative function parameters are represented as genuine continuation binders during lowering
- [x] Add lowering tests proving that `Decl::Fn` parameters marked `is_continuation` become μ/co-abstraction binders, not ordinary λ binders
- [x] Audit and remove parser behavior that derives parameter polarity from a lookahead arrow rather than from explicit declaration structure
- [x] Ensure a negative function is represented as a negative function rather than a value-returning `fn`

### Checklist

- [x] Keep a single `fn` declaration surface in the AST
- [x] Represent parameter and return polarity explicitly in the AST
- [x] Reject `+fn` and `-fn` prefixes with a migration diagnostic
- [x] Require either `->` for a positive function or `<-` for a negative function
- [x] Update parsing and tokenization
- [x] Update lowering for both forms
- [x] Update type inference for continuation return types
- [x] Update polarity checking
- [x] Update linearity checking
- [x] Add parser tests for negative functions using `<-`
- [x] Add parser tests rejecting `+fn` and `-fn` prefixes
- [x] Add lowering tests
- [x] Add inference tests
- [x] Add runtime tests
- [x] Rewrite examples using arrow-annotated `fn ... -> ...`
- [x] Rewrite continuation-taking helpers as either positive `->` functions or negative `<-` functions, according to meaning
- [x] Update `DESIGN.md`

---

## 4. Change `mu` Parameter Syntax

### Target syntax

```sl
mu route(x: +i32) | (k: -i32) {
    k(x)
}
```

Separate groups make the positive and negative binders visually distinct without directional keywords. An optional `-> ⊥` annotation may be used without changing lowering.

### Checklist

- [x] Decide that `mu(value_params) | (continuation_params)` uses explicit empty parenthesized groups; no bare `mu |` form is accepted
- [x] Parse the value-parameter group followed by `|` and the continuation-parameter group
- [x] Permit an empty value group, an empty continuation group, or both
- [x] Preserve bottom/command return semantics
- [x] Add explicit `-> ⊥` support without changing lowering
- [x] Reject a non-bottom `mu` return annotation
- [x] Update AST
- [x] Update lowering
- [x] Update inference
- [x] Update polarity checking
- [x] Update linearity checking
- [x] Update examples
- [x] Add parser tests for value-only, continuation-only, and both parameter groups
- [x] Add migration tests rejecting old `from` and `to` parameter syntax
- [x] Add runtime test proving continuation activation escapes the `mu`
- [x] Replace fuel-derived `μ` escape identifiers with stable unique identifiers
- [x] Add nested-μ escape tests proving each escape unwinds to exactly its own binder
- [x] Remove old parameter grammar and tests
- [x] Update `DESIGN.md`
---

## 5. Redesign `select`: Enum-to-Continuation

### Semantics

`select` no longer declares a type. It consumes an existing enum type and returns a continuation.

```sl
enum Color { Red, Green, Blue }

fn k(return: -i32) <- Color {
    select Color {
        return(0) => Red,
        return(1) => Green,
        return(2) => Blue,
    }
}
```

The `select Color { ... }` expression itself has the negative additive dual type of `Color`. Each arm must cover exactly one enum variant and must be exhaustive.
The negative function receives the consumer continuation `return`; activating the expression with an enum value selects the matching arm and supplies that enum value to the consumer.

### Required implementation changes

The implemented `select` still needs the following adjustments to match the final design:

- [x] Remove any residual type-definition behavior from `select`; it must reference an existing enum only
- [x] Make `select Color { ... }` an expression returning a continuation
- [x] Remove special `let` continuation-scope threading for `select` values; the expression itself denotes the continuation
- [x] Remove duplicated select-specific handling from both expression-level `let` lowering and block-level bodyless-`let` lowering
- [x] Document the reserved status of `return` and its use as a continuation parameter name in ordinary call syntax
- [x] Add parser tests for `return` as parameter name, expression callee, expression argument, and struct name marker, and document any positions where it remains rejected
- [x] Replace lowering-global continuation tracking (`CONTINUATIONS`) with explicit lexical context derived from binder polarity
- [x] Add lowering tests for nested continuation scopes, nested local `mu`, and selected error propagation
- [x] Verify block-sequence lowering names (`__seq*`, `__ret*`) cannot capture or collide with user-defined continuation names
- [x] Add lowering tests for nested blocks, shadowed user names, and empty or single-expression blocks
- [x] Support ordinary continuation application syntax: `cont(Color::Red)`
- [x] Restrict arm left-hand sides to consumer activation expressions over enum variants, rather than arbitrary positive values
- [x] Add parser/checker tests rejecting non-activation or non-variant `select` arm left-hand sides
- [x] Add exhaustive checking: every enum variant must have exactly one arm
- [x] Reject duplicate arms
- [x] Reject unknown variants
- [x] Verify that constructing `select` does not activate any `EXIT` arm; activation is lazy
- [x] Add a runtime test proving that activating one `select` arm does not activate the other arms
- [x] Define core lowering for `select` as a negative additive consumer
- [x] Update inference so `select EnumType` has the dual type of `EnumType`
- [x] Add expression inference coverage for `select` (the current inference module contains no `Expr::Select` case)
- [x] Strengthen polarity checking for `select` expressions beyond checking arm subexpressions
- [x] Add lowering tests that assert the negative-additive core representation, not merely the current `__select` builtin marker
- [x] Update linearity checking so a consumer used once in each mutually exclusive `select` arm counts as one use
- [x] Add parser tests for final `select` consumer syntax
- [x] Add lowering tests for final `select` consumer syntax
- [x] Add inference tests
- [x] Add exhaustiveness tests for exhaustive, missing, duplicate, and unknown variants
- [x] Add runtime tests:
  - `return(0)` receives `Color::Red`
  - `return(1)` receives `Color::Green`
  - `return(2)` receives `Color::Blue`
- [x] Replace the former experimental selection example with an enum/continuation `select` example
- [x] Remove `choose T { Variant }` syntax, lowering, checker support, and tests
- [x] Remove the incomplete `choose` design from the plan while its design is deferred

### Checklist

- [x] Implement `select` linearity checking: a consumer used in mutually exclusive arms counts as one use
- [x] Implement final `select` syntax and AST
- [x] Implement continuation application/dispatch
- [x] Implement exhaustive checking
- [x] Implement inference and type lowering
- [x] Implement runtime representation and activation
- [x] Add tests
- [x] Update examples
- [x] Update `DESIGN.md`

---

## 6. Continuation-Based Error Handling

The parser and other fallible operations should not return `Result`. They should offer their results to continuations.

### Target

- Parsing functions expose success and failure continuations.
- Success/failure selection can use final `select` over an appropriate enum.
- No `Result` enum is required.
- Error propagation must remain explicit and linear.

### Checklist

- [x] Define the canonical continuation-based error API
- [x] Rewrite the JSON parser without `Result`
- [x] Reassess the JSON parser after final `select` inference and core lowering; remove selected-continuation `?` sugar if it is no longer needed
- [x] Define the precise core semantics of explicit `e?` versus named `e?k` error propagation
- [x] Define which identifiers are accepted immediately after `?`, including reserved words such as `return`, and document the decision
- [x] Add lowering tests for both error-propagation forms, including nested propagation and interaction with current continuation scope
- [x] Add tests for successful parse
- [x] Add tests for malformed JSON
- [x] Add tests for trailing characters and trailing commas
- [x] Ensure linearity checks reject missing error continuation use
- [x] Ensure polarity checks reject positive parameters used as continuations
- [x] Update `DESIGN.md`
- [x] Decide whether `?` remains supported, is redesigned, or is removed
- [x] If `?` remains, document its precise continuation semantics

---

## 7. JSON Parser Migration

The example is continuation-oriented and will be revisited after final `select` inference and lowering are settled.

### Checklist

- [x] Migrate every bare `fn` to arrow-annotated `fn ... -> ...`
- [x] Migrate every `mu` declaration to `mu(values) | (continuations)`
- [x] Replace selected-continuation plumbing with final `select` once its inference and core lowering are settled; use a future negative multiplicative construct only if one is accepted
- [x] Keep behavior: valid JSON is printed, malformed JSON exits through the error continuation
- [x] Add a test that the JSON parser preserves output and exit status after selected-continuation plumbing is replaced
- [x] Remove any `Result`-like convention
- [x] Re-run full example
- [x] Add focused regression tests for parsing edge cases, including nested values, whitespace boundaries, malformed escape sequences, and unterminated literals
- [x] Update comments to describe final continuation semantics

---

## 8. Core Calculus and Documentation

### Checklist

- [x] Define the final core grammar after type-polarized `fn`, new `mu`, final `select`, and any future negative multiplicative construct
- [x] Decide whether `Command<I, O>` remains an accepted surface type former or is replaced by explicit negative function/continuation types
- [x] Remove the former `Command<I, O>` surface type former and document the explicit `(-I ⅋ O)` replacement
- [x] Add parser/lowering tests for the accepted continuation type formers after removing `Command<I, O>`
- [x] Define lowering rules for every accepted surface construct
- [x] Decide whether expression-level `dual(e)` remains part of the accepted language
- [x] Remove expression-level `dual(e)` from the accepted language; type-level `dual(A)` remains available
- [x] Add parser/lowering tests proving expression-level `dual(e)` is rejected while type-level `dual(A)` remains supported
- [x] Define core pretty-printing and round-trip tests, including declarations, cuts, `mu`, `select`, and polarity annotations
- [x] Add a complete lowering table covering declarations, expressions, cuts, `mu`, `select`, error propagation, and the surface interaction operator
- [x] Add tests for interaction/cut lowering with named consumers, compound consumers, and values of each polarity
- [x] Add tests that every documented lowering-table row corresponds to an actual lowering implementation
- [x] Rewrite `DESIGN.md` around λ̄μμ̃ and the final syntax
- [x] Document positive/negative additive data: `enum`/`match` and `select`/enum-continuation
- [x] Document positive multiplicative data: `struct`/`⊗`
- [x] Decide and document the accepted surface syntax for explicit tensor (`⊗`) and par (`⅋`) types, including their relationship to `struct` and any future negative multiplicative construct
- [x] Add parser, inference, polarity, and lowering tests for every accepted explicit connective type
- [x] Decide whether direct additive core connectives require surface syntax beyond `enum`/`match` and `select`, and document the decision
- [x] Define the type-lowering semantics of struct declarations, including their field types and tensor representation
- [x] Add inference and type-checking rules for struct literals and field patterns, including field presence, order, and type equality
- [x] Add lowering tests proving that struct literals lower to the documented tensor representation
- [x] Replace the placeholder struct/enum declaration type `Type::One` with precise declared types or a documented named-type representation
- [x] Add declaration-type tests for struct and enum names, including polarity and rejection of misuse as values without a proper constructor or literal
- [x] Define precise enum declaration and variant types, including payload arity and payload types, instead of reducing enum names to `Type::One`
- [x] Add inference and exhaustive-checking tests for enum variants with payloads, including payload arity and type mismatches
- [x] Decide and document the negative multiplicative surface construct: the surface exposes none, and `DESIGN.md` records the reason and the criteria a future design must meet
- [x] Document `EXIT: -i32`
- [x] Decide the role of the prelude `Result` and `Option` enums now that error handling is continuation-based; remove them if no longer canonical
- [x] Document migration from:
  - bare `fn` → `fn ... -> ...`
  - `+fn` → `fn ... -> ...`
  - `-fn k(K) -> (K₁, …, Kₙ)` → `fn k(K₁, …, Kₙ) <- K`
  - `command ...` → `mu ...`
  - `mu(x, to k)` → `mu(x) | (k)`
  - old experimental `to k` parameter markers → continuation group in `mu(...) | (...)`
  - old experimental `select` → final `select`
  - `agent.to(k, h)` → `agent.consume(k, h)`
  - removed `spawn`
  - removed expression-level value-returning `mu`
- [x] Remove stale concurrency references from `DESIGN.md`
- [x] Remove stale `command` and `spawn` references from `DESIGN.md`
- [x] Explain why the surface is Rust-like and asymmetric even though the calculus is symmetric

---

## 9. Implementation Phases

### Phase 0: Decide how to handle the current working tree

The working tree contains the redesigned `select` implementation and partially completed negative-function semantics. Continue validating the implementation against this plan.

- [x] Remove the surface `spawn` declaration
- [x] Rename the surface `command` declaration to `mu`
- [x] Remove the old expression-level/value-returning `mu`
- [x] Allow continuation parameters in ordinary `fn`
- [x] Add top-level `EXIT: -i32`
- [x] Add initial parser/checker/runtime test coverage for these features
- [x] Transform the superseded `select` work into the final design
- [x] Fix `examples/mu_escape.sl` to use local `mu escape() | (k: -i32)` syntax
- [x] Verify `examples/bottom_type.sl`, `examples/mu.sl`, and every other example
- [x] Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --workspace`
- [x] Run every example and record expected intentional failures
- [x] Ignore `instructions.txt`, `.#instructions.txt`, editor backup files, and other generated artifacts so they stay out of every commit

### Phase 1: Parameter syntax

- [x] Remove `from` and `to`
- [x] Implement `mu(values) | (continuations)`
- [x] Add optional `-> ⊥`
- [x] Update examples and tests
- [x] Add migration diagnostics
- [x] Validate with `cargo fmt --check`, clippy, workspace tests, and examples

### Phase 2: Polarize functions by types

- [x] Implement positive functions as `fn ... -> ...`
- [x] Implement negative functions as `fn(continuation_params) <- ContinuationType`
- [x] Require `->` for positive functions and `<-` for negative functions
- [x] Reject bare `fn`
- [x] Update AST, parsing, tokenization, polarity checking, examples, and tests
- [x] Reject `+fn` and `-fn` prefixes
- [x] Add continuation-return-type inference
- [x] Validate with fmt, clippy, workspace tests, and examples

### Phase 3: Final `select`

- [x] Redesign the existing `select` implementation into enum-to-continuation
- [x] Support `cont(Color::Red)`
- [x] Add exhaustive checking
- [x] Add linearity checking
- [x] Replace tests and examples
- [x] Remove `choose` from the plan while its design is deferred

### Phase 4: Error handling and JSON

- [x] Define canonical continuation error API
- [x] Rewrite the JSON parser
- [x] Add success and malformed-input regression tests
- [x] Decide the fate of `?` sugar

### Phase 5: Documentation and acceptance

- [x] Rewrite `DESIGN.md` as a standalone language reference
- [x] Add migration guide
- [x] Add final grammar and lowering table
- [x] Remove stale references, including internal parser test names that still say `command` and the former `examples/command.sl` filename/comment
- [x] Run the full acceptance suite: fmt, clippy, workspace tests, example suite, and documented intentional failures
- [x] Document the compiler diagnostic categories (`type`, `polarity`, `linearity`, `exhaustiveness`, `parse`, and `lowering`) and their precedence
- [x] Add driver tests proving checker diagnostics include source locations and remain stable across phases

---

## 10. Acceptance Criteria

### Language shape

- [x] No `spawn` exists
- [x] No `command` keyword exists
- [x] No `from` or `to` keyword exists
- [x] Bare `fn` without `->` or `<-` is rejected
- [x] `+fn` and `-fn` prefixes are rejected
- [x] `fn ... -> ...` is the value-to-value function form
- [x] Negative `fn(continuation_params) <- ContinuationType` has continuation parameters and produces a continuation
- [x] `mu` uses separate value and continuation parameter groups
- [x] `mu` returns a command/bottom and may annotate it `-> ⊥`
- [x] Final `select` consumes an existing enum and returns a continuation
- [x] Final `select` arms are exhaustive and unique
- [x] `EXIT: -i32` terminates the program

### Calculus/documentation

- [x] Every core λ̄μμ̃ construct has a documented surface representation or an explicit decision that it is not exposed in the surface language
- [x] Every accepted surface construct has a documented lowering
- [x] Every core term, co-term, command, and type in the final grammar has a precise surface mapping and lowering rule
- [x] Positive and negative additive data are documented
- [x] Positive multiplicative data is documented, and the negative multiplicative design is decided (deferred, with documented criteria)
- [x] `DESIGN.md` no longer discusses concurrency as a language feature
- [x] A migration guide exists

### Quality

- [x] `cargo fmt --check` passes
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo test --workspace` passes
- [x] Add a repository example-suite test that runs every example, records expected success/failure, and prevents silent regressions
- [x] Every non-error example runs successfully
- [x] Every intentional error example fails with the expected diagnostic
- [x] Add explicit expected-output assertions for example programs rather than only asserting process success
- [x] Define whether `main`’s final value is printed automatically, and separate program output from final-result output: the entry point is a command, so there is no final value — output is what the program prints
- [x] Add driver tests proving a program's output is exactly what it prints, in order
- [x] Add explicit expected-error assertions for intentional error examples rather than only asserting process failure
- [x] Define the accepted entry-point form: `mu main() | (exit: -i32)`, a command whose exit continuation carries the process status
- [x] Replace string-parsing of runtime `exit(...)` results with a structured exit result in the driver
- [x] Add driver tests for successful exit codes, nonzero exit codes, and diagnostics that merely look like exit strings
- [x] Add driver tests for missing `main`, malformed `main`, and each accepted entry-point return form
- [x] No stale `instructions.txt` or editor backup files are committed
