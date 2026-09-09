# Slant Redesign: λ̄μμ̃ Calculus

## Goal

Realign Slant with the lambda-bar-mu-mu-tilde calculus (λ̄μμ̃) rather than the looser “symmetric lambda calculus” formulation currently in the compiler. The redesign must preserve the ergonomic Rust-like surface syntax, keep values and continuations syntactically symmetric, and eliminate constructs that do not map cleanly onto λ̄μμ̃.

---

## 1. Terminology and Core Mapping

### Current state

| Surface construct | Current meaning | λ̄μμ̃ idea |
|---|---|---|
| `fn` | producer / lambda | `λx.t` |
| `mu` | producer that captures a return continuation | `μ`-like escape, but not the standard calculus binder |
| `command` | consumer / producer interaction with continuation parameters | a mixture of `μ` and `μ̃` |
| `spawn` | concurrency-like process creation | not part of λ̄μμ̃ |

### Target mapping

| Surface construct | λ̄μμ̃ core |
|---|---|
| `fn(x: +A) -> B` | `λx.t` |
| `fn(k: -A) -> B` | continuation-consuming producer; equivalent to `v · e` |
| `mu(x: +A) -> B` | `μ̃`-style consumer abstraction / consumer binder |
| `mu(k: -A) -> B` | `μ`-style continuation abstraction / producer binder |
| `e1 ∥ e2` | cut `⟨e1 ∥ e2⟩` |

### Checklist

- [ ] Audit every current surface construct and record its current λ̄μμ̃ meaning
- [ ] Define the final core grammar for Slant after the redesign
- [ ] Define the surface-to-core lowering rules
- [ ] Define the core pretty-printer and round-trip tests
- [ ] Update `DESIGN.md` to describe λ̄μμ̃ rather than “symmetric lambda calculus”

---

## 2. Replace `mu` Value-Return Blocks with Continuation-Taking `fn`

### Problem

A block that provides a value and captures a continuation is currently written as:

```sl
mu(ret: -i32) {
    ...
    ret(value)
}
```

In λ̄μμ̃ this is better understood as `v · e`: a producer paired with a continuation. The continuation parameter is not a special control construct; it is simply another function parameter.

### New rule

A continuation is a first-class function-like value. Therefore:

```sl
fn body(ret: -i32) -> i32 {
    ...
    ret(value)
}
```

has the same meaning. The existing `mu(ret: -i32) { ... }` form is removed rather than duplicated.

### Checklist

- [ ] Treat `fn` parameters with negative type as continuation parameters
- [ ] Allow negative parameters in ordinary `fn` declarations
- [ ] Remove special lowering for value-returning `mu(ret: -T)`
- [ ] Rewrite all examples that use `mu(ret: -T) { ... }`
- [ ] Update checker diagnostics that refer to `mu`
- [ ] Add tests for `fn` with continuation parameters
- [ ] Add lowering tests proving `fn(k: -T)` maps to `v · e`
- [ ] Update the JSON parser example to use continuation-taking `fn`

---

## 3. Rename `command` to `mu`

### Problem

`command` is the genuine λ̄μμ̃ control construct. It binds both positive values and negative continuations and performs a cut. Calling it `command` obscures its relationship to `μ` and `μ̃`.

### New syntax

Rename the keyword:

```sl
mu route(x: +i32, to k: -i32) {
    k(x)
}
```

The `to` marker remains because it distinguishes:

- positive value parameters (`x: +i32`)
- negative continuation parameters (`k: -i32`)

### Semantics

The declaration lowers to a `μ̃` abstraction over value parameters and a `μ` abstraction over continuation parameters.

### Checklist

- [ ] Rename the `command` keyword in the lexer and parser
- [ ] Rename `Decl::Command` in the AST to `Decl::Mu`
- [ ] Rename parser methods and diagnostics
- [ ] Update lowering from `Decl::Command` to `Decl::Mu`
- [ ] Update polarity checking for the renamed declaration
- [ ] Update linearity checking for the renamed declaration
- [ ] Update exhaustive checking if it inspects command declarations
- [ ] Update all examples from `command` to `mu`
- [ ] Update all tests from `command` to `mu`
- [ ] Remove any backward compatibility alias for `command`
- [ ] Add a migration note explaining that old `command` syntax is rejected

---

## 4. Abandon `spawn`

### Problem

`spawn` is not part of λ̄μμ̃. It introduces concurrency-like behavior that is outside the calculus and makes the semantics harder to define.

### Decision

Remove `spawn` entirely. There will be no replacement keyword in v0.2. Any future process-like extension must be designed separately and must not compromise the λ̄μμ̃ core.

### Checklist

- [ ] Remove `Expr::Spawn` from the AST
- [ ] Remove `spawn` keyword handling from the parser
- [ ] Remove `spawn` lowering logic
- [ ] Remove `spawn` polarity checking
- [ ] Remove `spawn` linearity checking
- [ ] Remove `spawn` tests
- [ ] Add a parser test that rejects the `spawn` keyword
- [ ] Search the repository for stale `spawn` references and remove them

---

## 5. Continuation `if` and `match`

### Problem

The current `if` and `match` are producer-oriented: they choose a value and return it to the current continuation. There is no symmetric consumer-oriented construct that chooses which continuation to activate.

### Symmetry requirement

For every producer construct, there should be a corresponding consumer construct.

### Proposed continuation `if`

Use `if` with continuation arms:

```sl
if value {
    ok => ...,
    err => ...,
}
```

A more explicit, polarity-safe syntax is:

```sl
if value to {
    ok => ...,
    err => ...,
}
```

Meaning:

- `value` is evaluated to a boolean
- one of the continuation arms is selected
- the selected continuation is activated with the supplied argument

A dual form is:

```sl
if value from {
    ok => ...,
    err => ...,
}
```

The dual selects the continuation to which control should return.

### Proposed continuation `match`

```sl
match value to {
    Pattern => continuation,
    Pattern => continuation,
}
```

The dual form is:

```sl
match value from {
    Pattern => continuation,
    Pattern => continuation,
}
```

### Naming decision

Use `to` for producer-to-consumer selection and `from` for consumer-to-producer selection.

### Checklist

- [ ] Choose final syntax for continuation `if`
- [ ] Choose final syntax for continuation `match`
- [ ] Add AST nodes for continuation conditionals
- [ ] Add AST nodes for continuation matches
- [ ] Update the parser with `to` and `from` forms
- [ ] Define lowering for continuation `if`
- [ ] Define lowering for continuation `match`
- [ ] Update the surface type checker
- [ ] Update polarity checking
- [ ] Update linearity checking
- [ ] Update exhaustiveness checking for continuation `match`
- [ ] Add parser tests
- [ ] Add lowering tests
- [ ] Add type-checker tests
- [ ] Add runtime tests
- [ ] Rewrite error-handling examples to use continuation `match`

---

## 6. Dual of `enum` and `struct`

### Problem

The language currently has positive `struct` and positive `enum`:

- `struct` is a tensor-like positive product
- `enum` is a positive additive sum

To preserve symmetry, the language needs:

- a negative product, dual to `struct`
- a negative sum, dual to `enum`

### Positive constructs

```sl
struct Pair {
    first: +i32,
    second: +i32,
}

enum Shape {
    Circle(+i32),
    Square(+i32),
}
```

### Negative product

A negative struct represents a continuation that consumes fields together. Proposed syntax:

```sl
struct NegPair {
    from first: -i32,
    from second: -i32,
}
```

This is the par-like dual of the positive struct.

### Negative sum

A negative enum represents a choice between continuations. Proposed syntax:

```sl
enum Result {
    from ok: -String,
    from err: -String,
}
```

This is the additive dual of the positive enum.

### Alternative syntax

Instead of `from`, use explicit polarity on fields:

```sl
struct -Pair {
    first: -i32,
    second: -i32,
}
```

The `from` marker is preferred because it is consistent with the continuation selection syntax in `if` and `match`.

### Checklist

- [ ] Choose final syntax for negative struct
- [ ] Choose final syntax for negative enum
- [ ] Add AST nodes for negative struct declarations
- [ ] Add AST nodes for negative enum declarations
- [ ] Update parser for negative struct declarations
- [ ] Update parser for negative enum declarations
- [ ] Update lowering for negative struct
- [ ] Update lowering for negative enum
- [ ] Update type lowering
- [ ] Update type pretty-printing
- [ ] Update inference
- [ ] Update polarity checking
- [ ] Update linearity checking
- [ ] Update exhaustiveness checking
- [ ] Define construction syntax for negative structs
- [ ] Define construction syntax for negative enums
- [ ] Define destruction syntax for positive structs and enums
- [ ] Add parser tests
- [ ] Add lowering tests
- [ ] Add type-checker tests
- [ ] Add runtime tests
- [ ] Rewrite the JSON parser example using negative enums for success and failure continuations

---

## 7. Builtin Continuation `EXIT: -i32`

### Purpose

The language needs a top-level continuation that can terminate a program with an integer status code.

### Syntax

```sl
EXIT(0)
```

### Type

```sl
EXIT: -i32
```

### Semantics

Activating `EXIT` terminates the current program with the supplied status code.

### Checklist

- [ ] Add `EXIT` to the builtins table
- [ ] Give `EXIT` type `-i32`
- [ ] Ensure `EXIT` is exempt from ordinary linearity rules
- [ ] Add a runtime test that `EXIT(0)` terminates successfully
- [ ] Add a runtime test that `EXIT(1)` terminates with failure
- [ ] Update examples to use `EXIT`
- [ ] Ensure `EXIT` is documented in `DESIGN.md`

---

## 8. Surface Syntax After Redesign

### Producer

```sl
fn add(x: +i32, y: +i32) -> i32 {
    x + y
}
```

### Producer with continuation parameter

```sl
fn parse(
    input: +String,
    ok: -String,
    err: -String,
) -> i32 {
    ...
}
```

### Consumer abstraction

```sl
mu parse(
    input: +String,
    to ok: -String,
    to err: -String,
) {
    ...
}
```

### Continuation conditional

```sl
if value to {
    ok => ...,
    err => ...,
}
```

### Continuation match

```sl
match value to {
    Number(n) => ...,
    Text(s) => ...,
}
```

### Negative struct

```sl
struct Handler {
    from ok: -String,
    from err: -String,
}
```

### Negative enum

```sl
enum Result {
    from ok: -String,
    from err: -String,
}
```

### Builtin exit

```sl
EXIT(0)
```

---

## 9. Implementation Phases

### Phase 1: Cleanup

- [ ] Remove `spawn`
- [ ] Rename `command` to `mu`
- [ ] Remove the old value-returning `mu` syntax
- [ ] Update examples
- [ ] Update tests

### Phase 2: Continuation Functions

- [ ] Allow continuation parameters in ordinary `fn`
- [ ] Define the lowering for continuation-taking `fn`
- [ ] Add tests for continuation-taking `fn`
- [ ] Rewrite JSON parser to use continuation-taking `fn`

### Phase 3: Builtin Exit

- [ ] Add `EXIT: -i32`
- [ ] Add runtime behavior
- [ ] Add tests
- [ ] Update examples

### Phase 4: Symmetric Control

- [ ] Implement continuation `if`
- [ ] Implement continuation `match`
- [ ] Add exhaustive checking for continuation `match`
- [ ] Add tests
- [ ] Rewrite error-handling examples

### Phase 5: Dual Data

- [ ] Implement negative struct
- [ ] Implement negative enum
- [ ] Implement construction and destruction forms
- [ ] Add tests
- [ ] Rewrite JSON parser using dual data

### Phase 6: Documentation

- [ ] Rewrite `DESIGN.md` around λ̄μμ̃
- [ ] Document the final grammar
- [ ] Document the mapping from surface syntax to core calculus
- [ ] Add a migration guide from the current syntax
- [ ] Remove stale concurrency references

---

## 10. Acceptance Criteria

- [ ] No `spawn` remains in the language
- [ ] `command` no longer exists as a keyword
- [ ] `mu` is the only keyword for consumer abstraction
- [ ] Ordinary `fn` can take continuation parameters
- [ ] Continuation `if` and `match` exist
- [ ] Negative `struct` and `enum` exist
- [ ] `EXIT: -i32` works as a top-level continuation
- [ ] Every core λ̄μμ̃ construct has a surface representation
- [ ] Every surface construct has a documented lowering to λ̄μμ̃
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] All examples run successfully
