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
- [ ] Sequent representations:
  - [ ] `Γ ⊢ t : A | Δ` (term judgment)
  - [ ] `Γ | e : A ⊢ Δ` (co-term judgment)
  - [ ] `c` (command judgment)
- [ ] Typing rules for all core constructors
- [ ] Bidirectional inference/check for terms
- [ ] Bidirectional inference/check for co-terms
- [ ] Context manipulation (add, remove, lookup)
- [ ] Test: well-typed terms accepted
- [ ] Test: ill-typed terms rejected

#### 5. Reduction
- [x] β-rule: `⟨ λx.t ∥ μ̃x.c ⟩ → c[t/x]`
- [x] μ-rule: `⟨ μα.c ∥ e ⟩ → c[e/α]`
- [x] co-β-rule: `⟨ t ∥ λ̄x.c ⟩ → c[t/x]`
- [x] Tensor projection: `⟨ (t1, t2) ∥ fst ⟩ → t1`, similarly `snd`
- [ ] Par elimination rules
- [x] Small-step semantics with reduction context
- [x] Normal-form detection
- [x] Fuel-based divergence detection
- [ ] Property test: type preservation for each rule
- [ ] Property test: confluence up to commuting conversions

**Deliverable:** well-typed core programs reduce to normal form (or diverge
detected by fuel). No parser required; tests construct ASTs directly.

## Phase 2: Surface syntax (2–3 weeks)

**Goal:** parse Rust-flavored Slant into the core IR.

### Lexer

- [ ] Token enum with spans
- [ ] Rust-like token set:
  - [ ] identifiers, keywords
  - [ ] integer literals (`i32`, `i64`, `u32`, `u64`)
  - [ ] float literals (`f32`, `f64`)
  - [ ] string literals (escape sequences)
  - [ ] char literals
  - [ ] boolean literals (`true`, `false`)
  - [ ] punctuation: `(`, `)`, `{`, `}`, `[`, `]`, `,`, `;`, `:`, `::`, `.`
  - [ ] operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`
  - [ ] interaction: `@`
  - [ ] arrow: `->`
  - [ ] polarity: `+Type`, `-Type`
  - [ ] Unicode operators: `⅋` (par), `⊗` (tensor)
- [ ] Comments: `//` line, `/* */` block (nested)
- [ ] Lexer error recovery: continue after invalid character

### Parser

- [ ] Hand-written recursive descent
- [ ] Precedence climbing for binary operators
- [ ] Pratt parsing for postfix/prefix operators
- [ ] Parse `fn` definitions and expressions
- [ ] Parse `mu` in three forms:
  - [ ] `mu(k: -A) { E }`
  - [ ] `mu() -> A { E }`
  - [ ] `mu(x: +A) -> B { E }`
- [ ] Parse `command` with `to` clauses
- [ ] Parse `match` expressions
- [ ] Parse `let` bindings
- [ ] Parse `if` / `else`
- [ ] Parse `struct` / `enum` declarations
- [ ] Parse `spawn` expressions
- [ ] Parse `dual` expressions
- [ ] Parse `@` interaction
- [ ] Parse partial application: `.to(k, h)` and `.partial(v)`
- [ ] Parse `?` operator
- [ ] Error recovery: synchronize on `;` and `}` after expression errors
- [ ] AST pretty-printer (used for round-trip testing)

### AST → core lowering

- [ ] Lowering context: variables, continuations, types
- [ ] `fn(x: +A) -> B { E }` → `λx. μα. E`
- [ ] `mu(k: -A) { E }` → `μα. E`
- [ ] `mu(x: +A) -> B { E }` → `λ̄x. ⟨ E ∥ α ⟩`
- [ ] `command f(x: +A, to k: -B) { E }` → `κx. μα. E`
- [ ] `f(a)` → `⟨ λ-bound f ∥ μ̃x. ... ⟩`
- [ ] `k(v)` → `Activate(k, v)`
- [ ] `match` → sum elimination via μ̃
- [ ] `?` → continuation split: `e(to current_ok, current_err)`
- [ ] `let` → let-binding via μ̃ over value
- [ ] `if/else` → sum elimination
- [ ] `spawn` → new command in multiset (deferred to runtime)
- [ ] `dual(e)` → polarity flip on terms/co-terms
- [ ] `@` interaction → cut
- [ ] Test: round-trip property `parse(print(lower(ast)))` α-equivalent

**Deliverable:** a `.sl` file can be parsed and lowered to well-typed core IR.

## Phase 3: Type checker (2 weeks)

**Goal:** full bidirectional checking with polarity and linearity.

### Polarity checking

- [ ] `fn` parameters must be `+`
- [ ] `mu` continuation ports must be `-`
- [ ] `command` value ports `+`
- [ ] `command` continuation ports `-`
- [ ] `dual` flips polarity
- [ ] `->` arrow desugaring checks source/target polarity
- [ ] Diagnostic: expected `+`, found `-` with span

### Linearity checking

- [ ] Every `+` variable used exactly once in its scope
- [ ] Every `-` continuation activated exactly once
- [ ] `Job` must be run exactly once
- [ ] `Service` must be invoked exactly once
- [ ] Partial application agents are linear
- [ ] Diagnostic: used 0 times / used 2 times, with span
- [ ] Diagnostic: dangling continuation on some path
- [ ] Exception: types annotated `Drop` may be discarded

### Match exhaustiveness

- [ ] Constructor extraction from `enum` declarations
- [ ] Coverage check: all constructors covered
- [ ] Wildcard `_` always allowed as fallback
- [ ] Guard clauses do not affect exhaustiveness
- [ ] Diagnostic: missing constructor in match

### Inference

- [ ] Bidirectional inference for `fn`, `mu`, `command`
- [ ] Type variables for generic functions
- [ ] Occurs check for recursive types
- [ ] Unification with polarity constraints
- [ ] Diagnostic: cannot infer type

**Deliverable:** well-typed programs accepted; ill-typed programs rejected
with good diagnostics. Property test: checker agrees with core IR checker.

## Phase 4: Interpreter (2 weeks)

**Goal:** tree-walking evaluator over the IR; correctness before performance.

### Values

- [ ] Value enum: closures, continuations, primitives, pairs, sums, thunks
- [ ] Environment as persistent map (chain of frames)
- [ ] Continuation values as closures over environment

### Evaluation

- [ ] Cut dispatch:
  - [ ] `⟨ λ ∥ μ̃ ⟩` β-rule
  - [ ] `⟨ μ ∥ e ⟩` μ-rule
  - [ ] `⟨ t ∥ λ̄ ⟩` co-β-rule
- [ ] `command` bodies evaluate to `Never`
- [ ] Activation transfers control
- [ ] `let` bindings
- [ ] `if/else` via sum elimination
- [ ] `match` via sum elimination
- [ ] Binary operators on primitives
- [ ] Comparison operators
- [ ] String operations
- [ ] Fuel counter for divergence detection (debug)

### Partial application

- [ ] `step.to(k, h)` returns a `Service` closure
- [ ] `f.partial(a)` returns a `Job` thunk
- [ ] `Service` invocation: wire ports, evaluate
- [ ] `Job.run()`: connect current continuation, evaluate
- [ ] Linear use enforcement at runtime (debug assertion)

### Error handling

- [ ] `?` lowers to continuation wiring
- [ ] No `Result` allocation in the happy path
- [ ] Multi-continuation operations dispatch directly

### Concurrency primitives

- [ ] `spawn` creates cooperative green thread
- [ ] Single OS thread, round-robin scheduler
- [ ] Yield points at cuts and channel operations

### Builtins

- [ ] `i32`/`i64`/`u32`/`u64` arithmetic
- [ ] Overflow checking (debug)
- [ ] `bool` operations
- [ ] `String` construction and formatting
- [ ] `println`
- [ ] Basic comparison

**Deliverable:** nontrivial programs run correctly, including `mu`-based
early exit and multi-continuation error handling.

## Phase 5: Concurrency and channels (2 weeks)

**Goal:** interaction-net-style concurrent semantics on a single scheduler.

### Channels

- [ ] Channel endpoints as dual co-variables
- [ ] `send` / `receive` as cuts
- [ ] Bounded channels
- [ ] Unbounded channels
- [ ] `select` over multiple channels

### Runtime

- [ ] `Multiset<Command>` as the program state
- [ ] Ready queue of active commands
- [ ] Round-robin scheduler
- [ ] Deterministic mode: fixed priority order
- [ ] Debug mode: race detection, dangling continuation detection
- [ ] `spawn` creates a new active command in the multiset
- [ ] Termination: multiset empty

### Deadlock detection

- [ ] Wait-for graph over agents
- [ ] Cycle detection
- [ ] Diagnostic: which agents are in the cycle
- [ ] Debug assertion in deterministic mode

### Testing

- [ ] Producer/consumer programs
- [ ] Pipeline programs
- [ ] Property test: scheduler-invariant results
- [ ] Property test: same final multiset regardless of interleaving

**Deliverable:** producer/consumer and pipeline programs run with
deterministic semantics.

## Phase 6: Standard library (2 weeks)

**Goal:** enough surface area to write realistic programs.

- [ ] Prelude:
  - [ ] `Option<T>`
  - [ ] `Result<T, E>` (boundary type only)
  - [ ] `List<T>`
  - [ ] `Map<K, V>`
  - [ ] `Set<T>`
  - [ ] `String`
  - [ ] `Command<I, O>` type former
- [ ] Multi-continuation error handling:
  - [ ] Integer parse (`to ok: -i64, empty: -String, overflow: -String`)
  - [ ] File read (success/not-found/permission)
  - [ ] Arithmetic overflow
- [ ] Channels:
  - [ ] `channel<T>()` creation
  - [ ] `send` / `receive`
  - [ ] `select`
- [ ] String formatting:
  - [ ] `println`
  - `format`
- [ ] File I/O:
  - [ ] Read file
  - [ ] Write file
  - [ ] Path manipulation
- [ ] FFI story decision (deferred; likely no FFI in v0.1)

**Deliverable:** a CLI program that reads files, parses them, and reports
errors via continuations.

## Phase 7: Compiler to interaction nets (3–4 weeks)

**Goal:** compile IR to Lafont-style interaction nets and execute them.

### Net representation

- [ ] Agent type (principal port + auxiliary ports)
- [ ] Net as multiset of agents + wiring
- [ ] Edge/port identity
- [ ] Net pretty-printer (graphviz)

### Compilation

- [ ] λ̄μμ̃ → interaction net compilation
- [ ] Tensor/par nodes
- [ ] Sum/product nodes
- [ ] Fan nodes for sharing

### Rewriting engine

- [ ] Deterministic rule priority
- [ ] Rewrite loop: find active pair, apply rule, repeat
- [ ] Normal form detection
- [ ] Fuel / step budget

### Optimization

- [ ] Lamping-style sharing with fan nodes
- [ ] Bracket / oracle correctness for optimal reduction
- [ ] Net simplification passes

### Materialization

- [ ] Net → runtime value conversion
- [ ] Builtin operations as net agents

### Benchmarking

- [ ] Benchmark suite: arithmetic, list operations, channels
- [ ] Compare against tree-walking interpreter
- [ ] Track regressions in CI

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
| M5 Concurrency | deterministic multiset semantics |
| M6 Stdlib | file-parsing CLI runs |
| M7 Nets | interaction-net backend works |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Confluence breaks with classical control | Restrict linearity; canonical rule priority; test corpus |
| Interaction nets too slow for v0.1 | Ship tree-walking interpreter first; nets are a later phase |
| Surface syntax drift before semantics is fixed | Freeze design per phase; changes via design notes |
| Error messages become unmanageable | Every phase ships diagnostics, not just acceptance |
| Continuation leak across `spawn` | Linearity checker rejects cross-spawn capture |

## Tooling

- [ ] `cargo fmt` in CI
- [ ] `cargo clippy -- -D warnings` in CI
- [ ] `cargo test` in CI
- [ ] `proptest` for duality and substitution laws
- [ ] `insta` for diagnostics and pretty-printed IR snapshots
- [ ] `tracing` spans around lowering, checking, reduction

## Immediate next step

Start Phase 0: create the workspace layout, empty crates, CI script, and
commit `DESIGN.md` as the v0.1 baseline.
