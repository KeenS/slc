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
mu route(x: +i32, to k: -i32) -> ⊥ {
    k(x)
}
```

The `to` marker remains because it distinguishes:

- positive value parameters (`x: +i32`)
- negative continuation parameters (`k: -i32`)

### Semantics

The declaration lowers to a `μ̃` abstraction over value parameters and a `μ` abstraction over continuation parameters.

Because a `mu` declaration denotes an activated command rather than a value-producing function, its return type is always `⊥`. Activating one of its continuations is the only way for control to leave the declaration; control never falls through the end of the body.

The explicit `-> ⊥` annotation is optional. Omitting it is equivalent to writing it.

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

## 5. Dual Control: `select` and `choose`

### Problem

The current `enum` and `match` are producer-oriented:

- `enum` constructs a positive additive sum
- `match` decomposes a positive additive sum by choosing one arm

That is only the positive half. Their exact duals are:

- `select` is the dual of `enum`
- `choose` is the dual of `match`

Keeping this symmetry is required. `enum` and `select` are additive type constructors; `match` and `choose` are their respective elimination forms.

### Symmetry requirement

For every producer construct, there should be a corresponding consumer construct:

| Positive | Polarity | Kind | Negative dual | Polarity | Kind |
|---|---|---|---|---|---|
| `enum` | positive additive | constructs one variant with `⊕` | `select` | negative additive | constructs one continuation with `&` |
| `match` | positive additive elimination | chooses one value arm | `choose` | negative additive elimination | offers one value to one of several continuations |

### `select`

`select` is the dual of `enum`. It constructs a negative additive sum, i.e. a continuation that can be activated in one of several ways. Proposed syntax:

```sl
select Color {
    EXIT(0) => Red,
    EXIT(1) => Green,
    EXIT(2) => Blue,
}
```

Each arm is a producer expression on the left and a variant name on the right. The result is a continuation of type `-Color`.

Meaning:

- the continuation is a negative additive sum
- each arm is a possible way to activate it
- activating the continuation with variant `Red` runs `EXIT(0)`
- activating with `Green` runs `EXIT(1)`

This is the negative additive dual of positive `enum`.

### `choose`

`choose` is the dual of `match`. It offers a value to one of several continuations. Proposed syntax:

```sl
choose Color { Red }  // exits with 0
```

Meaning:

- the value `Color_Red` is offered to the consumer
- the consumer selects one branch based on the variant
- control jumps to the corresponding continuation

This is the negative additive dual of positive `match`.

### Why `select` and `choose`

These keywords directly name the two directions of additive choice:

- `enum` **constructs** a positive choice
- `match` **eliminates** a positive choice
- `select` **constructs** a negative choice
- `choose` **eliminates** a negative choice

This is clearer than a generic `not` prefix because it names the actual operation, not merely the polarity.

### Checklist

- [ ] Add AST node for `select`
- [ ] Add AST node for `choose`
- [ ] Update the parser for `select`
- [ ] Update the parser for `choose`
- [ ] Define lowering for `select`
- [ ] Define lowering for `choose`
- [ ] Update type lowering
- [ ] Update type pretty-printing
- [ ] Update inference
- [ ] Update polarity checking
- [ ] Update linearity checking
- [ ] Update exhaustiveness checking for `choose`
- [ ] Add parser tests
- [ ] Add lowering tests
- [ ] Add type-checker tests
- [ ] Add runtime tests
- [ ] Rewrite error-handling examples to use `select` and `choose`
- [ ] Rewrite the JSON parser example using `select` and `choose`


---

## 6. Builtin Continuation `EXIT: -i32`

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

## 7. Surface Syntax After Redesign

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
) -> ⊥ {
    ...
}
```

### Negative additive construction

```sl
select Color {
    EXIT(0) => Red,
    EXIT(1) => Green,
    EXIT(2) => Blue,
}
```

### Negative additive elimination

```sl
choose Color { Red }  // exits with 0
```

### Builtin exit

```sl
EXIT(0)
```

---

## 8. Implementation Phases

### Phase 1: Cleanup

- [x] Remove `spawn`
- [x] Rename `command` to `mu`
- [ ] Remove the old value-returning `mu` syntax
- [ ] Update examples
- [ ] Update tests

### Phase 2: Continuation Functions

- [x] Allow continuation parameters in ordinary `fn`
- [x] Define the lowering for continuation-taking `fn`
- [x] Add tests for continuation-taking `fn`
- [ ] Rewrite JSON parser to use continuation-taking `fn`

### Phase 3: Builtin Exit

- [x] Add `EXIT: -i32`
- [x] Add runtime behavior
- [x] Add tests
- [ ] Update examples

### Phase 4: Negative Additive Control and Data

#### Negative additive construction

- [ ] Implement `select`
- [ ] Add exhaustive checking for `select`

#### Negative additive elimination

- [ ] Implement `choose`
- [ ] Add exhaustive checking for `choose`

#### Shared

- [ ] Add tests
- [ ] Rewrite error-handling examples

### Phase 5: Integrated Negative Additive Control

- [ ] Combine `select` and `choose` with continuation-taking `fn`
- [ ] Add exhaustiveness and linearity tests for the combined forms
- [ ] Rewrite JSON parser using `select` and `choose`

### Phase 6: Documentation

- [ ] Rewrite `DESIGN.md` around λ̄μμ̃
- [ ] Document the final grammar
- [ ] Document the mapping from surface syntax to core calculus
- [ ] Add a migration guide from the current syntax
- [ ] Remove stale concurrency references

---

## 9. Acceptance Criteria

- [ ] No `spawn` remains in the language
- [ ] `command` no longer exists as a keyword
- [ ] `mu` is the only keyword for consumer abstraction
- [ ] Ordinary `fn` can take continuation parameters
- [ ] Negative additive construction exists (`select`)
- [ ] Negative additive elimination exists (`choose`)
- [ ] `EXIT: -i32` works as a top-level continuation
- [ ] Every core λ̄μμ̃ construct has a surface representation
- [ ] Every surface construct has a documented lowering to λ̄μμ̃
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] All examples run successfully
