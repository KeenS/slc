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

## 5. Dual Control: Negative Multiplicatives and Additives

### Problem

The current `if` and `match` are producer-oriented:

- `if` chooses one of two value branches
- `match` chooses one of several value branches

That is only the positive half of control. We also need their exact duals:

- the dual of `if` is negative additive choice
- the dual of `match` is negative multiplicative decomposition

Keeping this symmetry is required. `if`, `match`, `struct`, and `enum` must form a polarity square.

### Symmetry requirement

For every producer construct, there should be a corresponding consumer construct:

| Positive construct | Polarity | Kind | Negative dual | Polarity | Kind |
|---|---|---|---|---|---|
| `if` | positive additive | selects one value branch | `if not` | negative additive | offers one value to one of two continuations |
| `match` | positive additive | selects one value arm | `match not` | negative additive | offers one value to one of several continuations |
| `struct` | positive multiplicative | bundles values with `⊗` | `struct not` | negative multiplicative | splits a value into multiple continuations with `⅋` |
| `enum` | positive additive | chooses one variant with `⊕` | `enum not` | negative additive | accepts one of several variants with `&` |

The key point is that `if` and `match` are additive control, while `struct` and `enum` are data. We need the negative forms of both.

### Negative multiplicative `if`

`if` is the simplest additive control form. Its negative dual should offer a value to one of two continuations. Proposed syntax:

```sl
if not value {
    zero => EXIT(0),
    nonzero => EXIT(1),
}
```

Meaning:

- `value` is evaluated to a boolean
- one of the two named continuations is selected
- the value is passed to the selected continuation

This is the negative additive dual of positive `if`.

### Negative multiplicative `match`

`match` should offer a value to one of several continuations. Proposed syntax:

```sl
match not value {
    Red => EXIT(0),
    Green => EXIT(1),
    Blue => EXIT(2),
}
```

Meaning:

- `value` is evaluated
- one of the patterns matches
- the corresponding continuation is activated with the bound values

This is the negative additive dual of positive `match`.

### Negative multiplicative struct

A positive `struct` bundles values together using `⊗`. Its dual should split a value into multiple continuations using `⅋`. Proposed syntax:

```sl
struct not Handler {
    ok: -String,
    err: -String,
}
```

Meaning:

- a value of type `Handler` is a continuation
- the continuation jointly consumes both fields
- the fields are not independently activated; they are consumed together as a `⅋`

This is the negative multiplicative dual of positive `struct`.

### Negative additive enum

A positive `enum` chooses one variant using `⊕`. Its dual should accept one of several variants using `&`. Proposed syntax:

```sl
enum not Result {
    ok: -String,
    err: -String,
}
```

Meaning:

- a value of type `Result` is a continuation
- the continuation accepts exactly one of the listed variants
- each variant is a distinct continuation branch

This is the negative additive dual of positive `enum`.

### Why `not`

`not` is already the natural polarity-flip operation in the language. Using it before a type or control keyword keeps the surface syntax readable while remaining unambiguous:

```sl
struct not T   // negative multiplicative
enum not T     // negative additive
if not e       // negative additive control
match not e    // negative additive control
```

This is clearer than reusing `+`/`-` on the declaration itself, because those are type polarities, not data declarations.

### Construction and destruction

For positive `struct` and `enum`, construction produces a value:

```sl
let p = Pair { first: 1, second: 2 };
let c = Shape::Circle(3);
```

For negative `struct` and `enum`, construction produces a continuation:

```sl
let h: Handler = Handler {
    ok: fn(value: +String) -> i32 { EXIT(0) },
    err: fn(message: +String) -> i32 { EXIT(1) },
};

let r: Result = Result::ok(
    fn(value: +String) -> i32 { EXIT(0) }
);
```

Destruction is by activation, not by destructuring:

```sl
h(value);         // negative struct: jointly consumes fields
r(value);         // negative enum: selects one variant
```

### Checklist

- [ ] Choose final syntax for negative `if`
- [ ] Choose final syntax for negative `match`
- [ ] Choose final syntax for negative `struct`
- [ ] Choose final syntax for negative `enum`
- [ ] Add AST nodes for negative conditionals
- [ ] Add AST nodes for negative matches
- [ ] Add AST nodes for negative struct declarations
- [ ] Add AST nodes for negative enum declarations
- [ ] Update parser for negative `if`
- [ ] Update parser for negative `match`
- [ ] Update parser for negative struct declarations
- [ ] Update parser for negative enum declarations
- [ ] Define lowering for negative `if`
- [ ] Define lowering for negative `match`
- [ ] Define lowering for negative struct
- [ ] Define lowering for negative enum
- [ ] Update type lowering
- [ ] Update type pretty-printing
- [ ] Update inference
- [ ] Update polarity checking
- [ ] Update linearity checking
- [ ] Update exhaustiveness checking for negative `match`
- [ ] Define construction syntax for negative structs
- [ ] Define construction syntax for negative enums
- [ ] Define destruction syntax for positive structs and enums
- [ ] Add parser tests
- [ ] Add lowering tests
- [ ] Add type-checker tests
- [ ] Add runtime tests
- [ ] Rewrite error-handling examples to use negative `struct`
- [ ] Rewrite error-handling examples to use negative `enum`
- [ ] Rewrite the JSON parser example using negative enums for success and failure continuations


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

### Negative conditional

```sl
if not value {
    zero => ...,
    nonzero => ...,
}
```

### Negative match

```sl
match not value {
    Red => ...,
    Green => ...,
}
```

### Negative struct

```sl
struct not Handler {
    ok: -String,
    err: -String,
}
```

### Negative enum

```sl
enum not Result {
    ok: -String,
    err: -String,
}
```

### Builtin exit

```sl
EXIT(0)
```

---

## 8. Implementation Phases

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

### Phase 4: Negative Multiplicative and Additive Control

#### Negative additive control

- [ ] Implement `if not`
- [ ] Implement `match not`
- [ ] Add exhaustive checking for `match not`

#### Negative multiplicative data

- [ ] Implement `struct not`
- [ ] Implement `enum not`
- [ ] Implement construction forms for negative data
- [ ] Implement destruction forms for negative data

#### Shared

- [ ] Add tests
- [ ] Rewrite error-handling examples

### Phase 5: Integrated Dual Data and Control

- [ ] Combine negative structs and enums with `if not` and `match not`
- [ ] Add exhaustiveness and linearity tests for the combined forms
- [ ] Rewrite JSON parser using negative data and negative control

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
- [ ] Negative multiplicative control exists (`if not`, `match not`)
- [ ] Negative additive data exists (`struct not`, `enum not`)
- [ ] `EXIT: -i32` works as a top-level continuation
- [ ] Every core λ̄μμ̃ construct has a surface representation
- [ ] Every surface construct has a documented lowering to λ̄μμ̃
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] All examples run successfully
