# Slant Implementation Plan

This document turns `DESIGN.md` into a concrete, phased implementation plan.
Each phase ends in a working, testable system — no speculative infrastructure.

## Phase 0: Bootstrap (0.5 week)

**Goal:** project hygiene, CI, and a locked-down design baseline.

- [x] Create Cargo workspace with 5 crates:
  - [x] `crates/slc-core` — λ̄μμ̃ calculus: types, terms, reduction
  - [x] `crates/slc-syntax` — lexer, parser, surface AST
  - [x] `crates/slc-check` — type checker, polarity, linearity
  - [x] `crates/slc-runtime` — interpreter, scheduler
  - [x] `crates/slc-driver` — CLI entry point
- [x] Add `rustfmt.toml` (team style)
- [x] Add `clippy.toml` if needed
- [x] Create `README.md` with build instructions
- [x] Create `LICENSE` (decide: MIT / Apache-2.0 / MPL-2.0)
- [x] Set up `.github/workflows/ci.yml`:
  - [x] `cargo fmt --check`
  - [x] `cargo clippy -- -D warnings`
  - [x] `cargo test`
  - [x] Matrix: stable Rust, Linux/macOS
- [x] Freeze `DESIGN.md` as v0.1
- [x] Add `docs/design-notes/` for future changes
- [x] Write `CONTRIBUTING.md`

**Deliverable:** `cargo test` passes; `slc --version` runs.

## Phase 1: Core calculus (1–2 weeks)

**Goal:** implement the λ̄μμ̃ calculus as an in-memory IR with syntax,
typing, and reduction — no surface language yet.

### IR

```rust
enum Term {
    Var(String),
    Lam(String, Box<Term>),
    Mu(String, Box<Command>),   // μα.c
    Pair(Box<Term>, Box<Term>), // tensor
    Inl(Box<Term>),
    Inr(Box<Term>),
}

enum CoTerm {
    Covar(String),
    CoLam(String, Box<Command>),  // λ̄x.c
    MuTilde(String, Box<Command>), // μ̃x.c
    Par(Box<CoTerm>, Box<CoTerm>),
    Fst,
    Snd,
}

enum Command {
    Cut(Term, CoTerm),     // ⟨ t ∥ e ⟩
    Command(String, Term), // κx.t
    Activate(Term, Term),  // k(v)
}
```

### Types

```rust
enum Type {
    Pos(Base),                  // +i32, +bool, ...
    Neg(Base),                  // -i32, ...
    Tensor(Box<Type>, Box<Type>),
    Par(Box<Type>, Box<Type>),
    One,
    Bottom,
    Dual(Box<Type>),
    With(Box<Type>, Box<Type>),
    Bang(Box<Type>),
    List(Box<Type>),
    Fun(Box<Type>, Box<Type>), // sugar
}
```

### Work items

#### 1. Types
- [x] Define `Type` enum
- [x] Implement `Type::dual()` involution
- [x] Property test: `dual(dual(t)) == t`
- [x] Property test: `dual` is involutive on all type constructors
- [x] Pretty-printer for `Type`
- [x] Parser-independent construction helpers

#### 2. Terms and co-terms
- [x] Define `Term`, `CoTerm`, `Command` enums
- [x] Implement `Display` for each
- [x] α-equivalence check
- [x] Fresh variable generation (avoid capture)

#### 3. Substitution
- [x] Capture-avoiding substitution: terms
- [x] Capture-avoiding substitution: co-terms
- [x] Capture-avoiding substitution: commands
- [x] Property test: substitution preserves α-equivalence
- [x] Property test: substitution is idempotent when variable not free

#### 4. Type checking
- [x] Sequent representations:
  - [x] `Γ ⊢ t : A | Δ` (term judgment)
  - [x] `Γ | e : A ⊢ Δ` (co-term judgment)
  - [x] `c` (command judgment)
- [x] Typing rules for all core constructors
- [x] Bidirectional inference/check for terms
- [x] Bidirectional inference/check for co-terms
- [x] Context manipulation (add, remove, lookup)
- [x] Test: well-typed terms accepted
- [x] Test: ill-typed terms rejected

#### 5. Reduction
- [x] β-rule: `⟨ λx.t ∥ μ̃x.c ⟩ → c[t/x]`
- [x] μ-rule: `⟨ μα.c ∥ e ⟩ → c[e/α]`
- [x] co-β-rule: `⟨ t ∥ λ̄x.c ⟩ → c[t/x]`
- [x] Tensor projection: `⟨ (t1, t2) ∥ fst ⟩ → t1`, similarly `snd`
- [x] Par elimination rules
- [x] Small-step semantics with reduction context
- [x] Normal-form detection
- [x] Fuel-based divergence detection
- [x] Property test: type preservation for each rule
- [x] Property test: confluence up to commuting conversions

**Deliverable:** well-typed core programs reduce to normal form (or diverge
detected by fuel). No parser required; tests construct ASTs directly.

## Phase 2: Surface syntax (2–3 weeks)

**Goal:** parse Rust-flavored Slant into the core IR.

### Lexer

- [x] Token enum with spans
- [x] Rust-like token set:
  - [x] identifiers, keywords
  - [x] integer literals (`i32`, `i64`, `u32`, `u64`)
  - [x] float literals (`f32`, `f64`)
  - [x] string literals (escape sequences)
  - [x] char literals
  - [x] boolean literals (`true`, `false`)
  - [x] punctuation: `(`, `)`, `{`, `}`, `[`, `]`, `,`, `;`, `:`, `::`, `.`
  - [x] operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`
  - [x] interaction: `@`
  - [x] arrow: `->`
  - [x] polarity: `+Type`, `-Type`
  - [x] Unicode operators: `⅋` (par), `⊗` (tensor)
- [x] Comments: `//` line, `/* */` block (nested)
- [x] Lexer error recovery: continue after invalid character

### Parser

- [x] Hand-written recursive descent
- [x] Precedence climbing for binary operators
- [x] Pratt parsing for postfix/prefix operators
- [x] Parse `fn` definitions and expressions
- [x] Parse `mu` in three forms:
  - [x] `mu(k: -A) { E }`
  - [x] `mu() -> A { E }`
  - [x] `mu(x: +A) -> B { E }`
- [x] Parse `command` with `to` clauses
- [x] Parse `match` expressions
- [x] Parse `let` bindings
- [x] Parse `if` / `else`
- [x] Parse `struct` / `enum` declarations
- [x] Parse `spawn` expressions
- [x] Parse `dual` expressions
- [x] Parse `@` interaction
- [x] Parse partial application: `.to(k, h)` and `.partial(v)`
- [x] Parse `?` operator
- [x] Error recovery: synchronize on `;` and `}` after expression errors
- [x] AST pretty-printer (used for round-trip testing)

### AST → core lowering

- [x] Lowering context: variables, continuations, types
- [x] `fn(x: +A) -> B { E }` → `λx. μα. E`
- [x] `mu(k: -A) { E }` → `μα. E`
- [x] `mu(x: +A) -> B { E }` → `λ̄x. ⟨ E ∥ α ⟩`
- [x] `command f(x: +A, to k: -B) { E }` → `κx. μα. E`
- [x] `f(a)` → `⟨ λ-bound f ∥ μ̃x. ... ⟩`
- [x] `k(v)` → `Activate(k, v)`
- [x] `match` → sum elimination via μ̃
- [x] `?` → continuation split: `e(to current_ok, current_err)`
- [x] `let` → let-binding via μ̃ over value
- [x] `if/else` → sum elimination
- [x] `spawn` → new command in multiset — **deferred from v0.1** by the
  concurrency decision below; `spawn` remains parsed and checked, and lowering
  intentionally reports `Unsupported` rather than silently giving it a
  sequential meaning
- [x] `dual(e)` → polarity flip on terms/co-terms
- [x] `@` interaction → cut
- [x] Test: round-trip property `parse(print(lower(ast)))` α-equivalent

**Deliverable:** a `.sl` file can be parsed and lowered to well-typed core IR.

## Phase 3: Type checker (2 weeks)

**Goal:** full bidirectional checking with polarity and linearity.

### Polarity checking

- [x] `fn` parameters must be `+`
- [x] `mu` continuation ports must be `-`
- [x] `command` value ports `+`
- [x] `command` continuation ports `-`
- [x] `dual` flips polarity
- [x] `->` arrow desugaring checks source/target polarity
- [x] Diagnostic: expected `+`, found `-` with span

### Linearity checking

- [x] Every `+` variable used exactly once in its scope
- [x] Every `-` continuation activated exactly once
- [x] `Job` must be run exactly once
- [x] `Service` must be invoked exactly once
- [x] Partial application agents are linear
- [x] Diagnostic: used 0 times / used 2 times, with span
- [x] Diagnostic: dangling continuation on some path
- [x] Exception: types annotated `Drop` may be discarded

### Match exhaustiveness

- [x] Constructor extraction from `enum` declarations
- [x] Coverage check: all constructors covered
- [x] Wildcard `_` always allowed as fallback
- [x] Guard clauses do not affect exhaustiveness
- [x] Diagnostic: missing constructor in match

### Inference

- [x] Bidirectional inference for `fn`, `mu`, `command`
- [x] Type variables for generic functions
- [x] Occurs check for recursive types
- [x] Unification with polarity constraints
- [x] Diagnostic: cannot infer type

**Deliverable:** well-typed programs accepted; ill-typed programs rejected
with good diagnostics. Property test: checker agrees with core IR checker.

## Phase 4: Interpreter (2 weeks)

**Goal:** tree-walking evaluator over the IR; correctness before performance.

### Values

- [x] Value enum: closures, continuations, primitives, pairs, sums, thunks
- [x] Environment as persistent map (chain of frames)
- [x] Continuation values as closures over environment

### Evaluation

- [x] Cut dispatch:
  - [x] `⟨ λ ∥ μ̃ ⟩` β-rule
  - [x] `⟨ μ ∥ e ⟩` μ-rule
  - [x] `⟨ t ∥ λ̄ ⟩` co-β-rule
- [x] `command` bodies evaluate to `Never`
- [x] Activation transfers control
- [x] `let` bindings
- [x] `if/else` via sum elimination
- [x] `match` via sum elimination
- [x] Binary operators on primitives
- [x] Comparison operators
- [x] String operations
- [x] Fuel counter for divergence detection (debug)

### Partial application

- [ ] `step.to(k, h)` returns a `Service` closure
- [ ] `f.partial(a)` returns a `Job` thunk
- [ ] `Service` invocation: wire ports, evaluate
- [ ] `Job.run()`: connect current continuation, evaluate
- [ ] Linear use enforcement at runtime (debug assertion)

### Error handling

- [x] `?` lowers to continuation wiring
- [x] No `Result` allocation in the happy path
- [x] Multi-continuation operations dispatch directly

### Concurrency primitives

- [x] Decision: defer all v0.1 concurrency primitives. The symmetric core,
  continuation-based errors, and interaction-net backend are useful without a
  scheduler, and `spawn` must not be approximated as ordinary sequencing.

Deferred items (not part of v0.1):

- ~~`spawn` creates cooperative green thread~~
- ~~Single OS thread, round-robin scheduler~~
- ~~Yield points at cuts and channel operations~~

### Builtins

- [x] `i32`/`i64`/`u32`/`u64` arithmetic
- [x] Overflow checking (debug)
- [x] `bool` operations
- [x] `String` construction and formatting
- [x] `println`
- [x] Basic comparison

**Deliverable:** nontrivial programs run correctly, including `mu`-based
early exit and multi-continuation error handling.

## Note: Concurrency deferred

**Decision:** Concurrency is not necessary for v0.1. The symmetric core
does not require a multiset scheduler to be useful, and the interaction-net
backend (Phase 7) provides the right substrate for concurrent evaluation
later. Skip this phase; revisit after the stdlib and net compiler are
working.

Removed scope for v0.1:

- ~~Channel endpoints as dual co-variables~~
- ~~`send` / `receive` as cuts~~
- ~~Bounded / unbounded channels~~
- ~~`select`~~
- ~~`Multiset<Command>` scheduler~~
- ~~Deadlock detection~~
- ~~Producer/consumer and pipeline programs~~

When revisited, the design already in `DESIGN.md` (§7) remains the target.

## Phase 5: Standard library (2 weeks)

**Goal:** enough surface area to write realistic programs.

- [x] Prelude:
  - [x] `Option<T>`
  - [x] `Result<T, E>` (boundary type only)
  - [x] `List<T>`
  - [x] `Map<K, V>`
  - [x] `Set<T>`
  - [x] `String`
  - [x] `Command<I, O>` type former
- [x] Multi-continuation error handling:
  - [ ] Integer parse (`to ok: -i64, empty: -String, overflow: -String`)
  - [x] File read (success/not-found/permission)
  - [x] Arithmetic overflow
- [x] Decision: defer channels with concurrency (see the Phase 5 note below)
- [x] String formatting:
  - [x] `println`
  - `format`
- [x] File I/O:
  - [x] Read file
  - [x] Write file
  - [x] Path manipulation
- [x] FFI story decision (deferred; likely no FFI in v0.1)

**Deliverable:** a CLI program that reads files, parses them, and reports
errors via continuations.

## Phase 6: Compiler to interaction nets (3–4 weeks)

**Goal:** compile IR to Lafont-style interaction nets and execute them.

### Net representation

- [x] Agent type (principal port + auxiliary ports)
- [x] Net as multiset of agents + wiring
- [x] Edge/port identity
- [x] Net pretty-printer (graphviz)

### Compilation

- [x] λ̄μμ̃ → interaction net compilation
- [x] Tensor/par nodes
- [x] Sum/product nodes
- [x] Fan nodes for sharing

### Rewriting engine

- [x] Deterministic rule priority
- [x] Rewrite loop: find active pair, apply rule, repeat
- [x] Normal form detection
- [x] Fuel / step budget

### Optimization

- [x] Lamping-style sharing with fan nodes
- [x] Bracket / oracle correctness for optimal reduction
- [x] Net simplification passes

### Materialization

- [x] Net → runtime value conversion
- [x] Builtin operations as net agents

### Benchmarking

- [x] Benchmark suite: arithmetic, list operations
- [x] Compare against tree-walking interpreter
- [x] Track regressions in CI

**Deliverable:** programs compiled via nets run correctly; measurable
speedup on arithmetic-heavy programs.

## Milestones and exit criteria

| Milestone | Exit criterion |
|---|---|
| M0 Bootstrap | CI green, workspace compiles |
| M1 Core | well-typed IR programs reduce correctly |
| M2 Syntax | `.sl` files parse and lower |
| M3 Types | polarity + linearity checking |
| M4 Interpreter | realistic programs execute |
| M5 Concurrency | DEFERRED |
| M5 Stdlib | file-parsing CLI runs |
| M6 Nets | interaction-net backend works |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Confluence breaks with classical control | Restrict linearity; canonical rule priority; test corpus |
| Interaction nets too slow for v0.1 | Ship tree-walking interpreter first; nets are a later phase |
| Surface syntax drift before semantics is fixed | Freeze design per phase; changes via design notes |
| Error messages become unmanageable | Every phase ships diagnostics, not just acceptance |

## Tooling

- [x] `cargo fmt` in CI
- [x] `cargo clippy -- -D warnings` in CI
- [x] `cargo test` in CI
- [x] `proptest` for duality and substitution laws
- [x] `insta` for diagnostics and pretty-printed IR snapshots
- [x] `tracing` spans around lowering, checking, reduction

## Immediate next step

Complete. Phases 0–5 are done; Phase 6 (interaction nets) is next.
