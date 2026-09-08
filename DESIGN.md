# Slant Language Design

Slant is a Rust-flavored programming language whose core semantics are based on
the symmetric lambda calculus (SLC) tradition, instantiated as a
λ̄μμ̃-style calculus (Curien–Herbelin). The surface language is intentionally
familiar to Rust programmers, while the underlying execution model is built
from symmetric interactions between dual processes.

## 1. Design goals

1. **Rust-like syntax** — familiar braces, `fn`, `let`, `match`, type
   annotations, and paths.
2. **Symmetric core** — programs are not "functions consume arguments," but
   pairs of dual processes that interact.
3. **Classical logic foundation** — the type system follows
   Barbanera–Berardi-style proof/refutation duality, operationalized through
   the λ̄μμ̃ machinery of terms, co-terms, and commands.
4. **Interaction semantics** — evaluation is a chemical/interaction-net style
   rewrite system, suitable for a concurrent runtime.
5. **Deterministic, analyzable** — symmetric classical calculi often risk
   non-confluence; the language layer must tame this with typing discipline and
   canonical interaction rules.

## 2. Conceptual model

Every value has a **dual**:

```text
dual(A ⊗ B) = dual(A) ⅋ dual(B)
dual(1)     = ⊥
dual(⊥)     = 1
dual(+A)    = -A
```

The language exposes this minimally; most programs look like ordinary Rust
code, and the symmetry appears only where needed.

A running program consists of:

```text
Program     ::= Command*
Command     ::= ⟨ Term ∥ CoTerm ⟩
Term        ::= ...          // proofs / values
CoTerm      ::= ...          // refutations / consumers
Cut         ::= ⟨ Term ∥ CoTerm ⟩
```

Evaluation proceeds by cuts — a proof meeting a refutation — rather than by
evaluating an application head. This is the λ̄μμ̃ structure: `λ` produces
values (positive), `μ` produces continuations (negative), and `cut` is the
primitive interaction.

## 3. Core syntax

### Functions and application

```rust
fn add(x: i32, y: i32) -> i32 { x + y }
fn main() { let z = add(1, 2); }
```

This desugars to symmetric interaction:

```text
add ⋈ (1, 2)
```

The arrow type is derived sugar:

```text
fn(A, B) -> C   ≡   dual(A) ⅋ dual(B) ⅋ C
```

So every "function" is genuinely a dual-pair participant, not a privileged
consumer.

### Symmetric binding: `fn` and `mu`

`fn` and `mu` are the surface spellings of the two abstractions in the core
calculus. They mirror each other structurally: binder, optional type
annotation, body.

```rust
fn(x: +i32) -> i32 { x + 1 }        // λ abstraction — proof side
mu(k: -i32) { ... }                  // μ abstraction — refutation side
```

- `fn` binds **value parameters** and produces a result.
- `mu` names the **current continuation** and captures it as a first-class
  value. A `mu` expression has type `A` when its body has type `A` and the
  bound name has type `-A`.

Continuation activation reuses ordinary call syntax:

```rust
mu(k: -i32) {
    if dry_run { k(0); }   // escape with 0; the rest never runs
    expensive_path()
}
```

In fact, `mu` **does** have a return type — it's just spelled implicitly
because the continuation determines it. These are equivalent:

```rust
mu(k: -i32) { ... }             // continuation form
mu() -> i32 { ... }             // result form, k inferred
```

The `-> T` form names the answer type directly; the continuation form names
the port that will receive it. The relationship is:

```text
mu(k: -T) { ... }  ≡  mu() -> T { ... }   // with k bound to the output port
```

When both a parameter and a result type appear, `mu` becomes a **continuation
producer** — the dual of a function:

```rust
mu(x: +A) -> B { ... }    // : +A -> +B, specialized to a continuation port
```

Read it as: "given a value of `+A`, compute the rest of the computation and
answer with `B`." It is the negative mirror of `fn(x: +A) -> B`.

```text
fn(x: +A) -> B { E }    // λ: waits for input, produces output
mu(x: +A) -> B { E }    // μ: takes input, hands control to the continuation
```

In λ̄μμ̃ terms, `mu(x: +A) -> B` corresponds to a co-abstraction: a co-term
that consumes a `+A` and produces a cut whose answer type is `B`. Where `fn`
is a proof looking for a refutation to interact with, `mu` is a refutation
looking for a proof.

The term-level involution makes the symmetry lexically visible:

```text
dual(fn(x) { t })  ≡  mu(x) { dual(t) }
```

`f @ g` pairs a λ-shaped term with a μ-shaped term; both spellings are
first-class. Ordinary application `f(a)` is the common case where the μ side
is the trivial wrapper `dual(a)`:

```text
f(a)  ≡  f @ dual(a)
```

### Commands: mixed abstractions

`fn` and `mu` are the pure polarities, but many processes naturally need
both: they receive values *and* own output continuations. The surface form for
this is `command`.

```rust
command step(x: +i32, to k: -i32, h: -String) {
    if x > 0 { k(x) } else { h("negative") }
}
```

Read it as an **agent definition** in the interaction net:

- value parameters (`x: i32`) are **input ports**;
- continuation parameters (`k`, `h`) are **output ports**;
- the body is a process that eventually places a value into one of its
  continuation ports.

A command body has type `Never`: every execution path must either diverge or
activate one of its continuations. The linear-use rules from §6 apply to each
continuation separately — captured exactly once, never duplicated, never
silently dropped on a terminating path.

The single-continuation case is definitional sugar:

```text
command f(x: A, to k: -B) { E }
≡
fn f(x: A) -> B { mu(k) { E } }
```

With several continuations, `command` is a genuine generalization: it names a
multi-port agent directly rather than encoding the extra ports as nested
`mu`/channel plumbing. Multiple continuations are equivalent to a single
continuation of sum type (`-A & -B ≡ -(A + B)`), but the
multi-port spelling is preferable when defining custom interaction rules.

Invocation:

```rust
let r = step(42);                  // exactly one continuation:
                                   // wires the current continuation
spawn step(42).to(k, h);         // explicit output ports
```

### Partial application in either polarity

Wiring order is an implementation detail of the surface syntax, not a semantic
constraint. An agent may have its output ports connected before its input
arrives, or its inputs connected before a consumer appears. Both leave a
partially wired agent — a value with one hole.

**Continuations first** produces a `Service`: a running-capable agent that is
waiting for its input.

```rust
let svc = step.to(k, h);   // Service<i32> — output ports wired
let r = svc(42);             // : Never — results arrive in k or h
```

```text
c.to(k1, ..., kn) : Service<I>     when c : Command<I, O...>
svc(v)               : Never        when svc : Service<I>
```

The call has type `Never` because the continuations were supplied explicitly:
results appear in `k` or `h`, not in the caller. When a command has exactly
one continuation, `svc` can still be re-connected to the current continuation:

```rust
let svc = step.to(current());  // Service<i32>
let r  = svc(42);                // : i32
```

**Values first** produces the dual construct, a `Job`: inputs are wired and
the agent is waiting for its consumer.

```rust
let job = f.partial(42);     // Job<i32> — input ports wired
let r   = job.run();         // : i32 — wires the current continuation
```

```text
f.partial(v1, ..., vn) : Job<B>     when f : fn(A, B) -> C
job.run()              : C         when job : Job<C>
```

`Service` and `Job` are dual hole types:

```text
dual(Service<A>) ≡ Job<dual(A)>
dual(Job<A>)     ≡ Service<dual(A)>
```

`f(a)` is sugar for `f.partial(a).run()` in the common case where both sides
are wired immediately; `step(a).to(k, h)` is sugar for
`step.to(k, h)(a)`.

Linearity applies to partially applied agents as well:

- a `Service` must receive exactly one input — otherwise its continuations are
  left dangling (compile-time warning, runtime deadlock detector);
- a `Job` must be run or explicitly cancelled — dropping one discards the
  work, so it is an error unless the type is annotated `Drop`.
- a partially applied agent is itself linear and cannot be invoked twice.

This is the ergonomic payoff of the net model: currying and co-currying are
not language features to be implemented, just two orders of connecting ports.

Type form, Rust-style:

```rust
let step: Command<i32, (i32, String)>;
//          ^ input      ^ output choices
```

`Command` is the point where the surface syntax becomes genuinely symmetric:
its parameter list already carries both polarities, so `fn` and `mu` are the
degenerate one-sided cases.

### Dual processes

Explicit symmetry appears via `dual` blocks:

```rust
let f: i32 -> i32 = fn(x) { x + 1 };
let g = dual f;  // g : dual(i32 -> i32)
```

Interaction is explicit with `f @ g`:

```rust
let result = f @ dual (1);  // result : i32
```

Think of `@` as "collide these dual processes and let them reduce."

## 4. Type system

Base layer is multiplicative classical linear logic, softened with Rust-like
ergonomics.

### Core types

```text
A, B ::= +i32 | +bool | +String
       | A ⊗ B        // pair (tensor)
       | A ⅋ B        // par (dual pair)
       | 1 | ⊥        // units
       | dual(A)      // explicit dual, only a sign flip
       | &A | Copy A  // sharing / replication
       | !A
       | [A]          // list
       | A -> B       // sugar for dual(A) ⅋ B
```

Every type carries an explicit polarity:

```text
+A    value (proof side)
-A    continuation (refutation side)
```

`dual` is not a wrapper; it is a sign operation:

```text
dual(+A)    = -A
dual(-A)    = +A
dual(A ⊗ B) = dual(A) ⅋ dual(B)
dual(1)     = ⊥
dual(⊥)     = 1
dual(+A + B)= dual(A) & dual(B)
```

The positive/negative spelling makes the symmetry total: every type has a
sign, every construct has a dual, and `dual` never wraps — it only flips.

### Derived sugar

```rust
struct Pair<A, B> { first: A, second: B }
// desugars to tensor: A ⊗ B

enum Option<T> {
    Some(T),
    None,
}
// desugars to T + 1 (classical additive sum)
```

Additives (`+`, `&`) are present in the core even though they're not strictly
part of minimal MELL — they make the language usable.

### Duality rules exposed to users

```rust
type Dual<T> = ...;  // compiler-known involution

dual(+i32) = -i32
dual(A ⊗ B) = dual(A) ⅋ dual(B)
dual(A + B) = dual(A) & dual(B)
dual(dual(A)) = A
```

## 5. Pattern matching

`match` is the main control structure. It compiles to symmetric interaction
between a sum and its dual context.

```rust
match x {
    Some(v) => v + 1,
    None => 0,
}
```

Desugars conceptually to:

```text
x ⋈ dual(match context)
```

The scrutinee and match branches are dual partners. This gives classical
control: branches are refutations, not just consumers.

Full Rust-style destructuring:

```rust
match point {
    Point { x: 0, y } => y,
    Point { x, y } => x * y,
}
```

## 6. Control flow and classical features

The single classical primitive is `mu`; it names the current continuation and
captures it as a first-class value.

```rust
let r = mu(k) {
    if condition {
        k(42);        // escape with the continuation
    }
    0
};
```

`call_cc` is derived, not primitive:

```rust
fn call_cc<A, B>(f: fn(A -> B) -> A) -> A {
    mu(k) { f(k) }
}
```

Its type is Peirce's law:

```text
((A -> B) -> A) -> A
```

This is where classical logic pays off: continuations come from the same
λ/μ symmetry that functions do, rather than being bolted onto an
intuitionistic core.

However, unrestricted classical control plus effects breaks confluence. Slant
restricts:

1. Continuations are **linear** — captured exactly once.
2. No continuation crosses a `spawn` boundary.
3. Interaction order is canonicalized by the runtime scheduler (see §8).

## 7. Concurrency

Symmetric semantics maps naturally to concurrent interaction nets. Expose it
with lightweight primitives:

```rust
let a = spawn { producer() };
let b = dual spawn { consumer() };
let result = a @ b;
```

- Every channel is a pair of dual endpoints.
- Sending on one endpoint is symmetric interaction with receiving on the other.
- No privileged sender/receiver at the semantic level.

Example:

```rust
fn producer(end: Dual<i32>) {
    send(end, 42);
}

fn consumer(end: i32) {
    let x = receive(end);
    println("got {}", x);
}

fn main() {
    let (a, b) = channel::<i32>();
    spawn producer(b);
    spawn consumer(a);
}
```

Under the hood, `(a, b)` is a dual pair of interaction-net agents, and
`send`/`receive` unify into an interaction rule.

`send` and `receive` are themselves sugar over the λ/μ pair:
`receive(c)` is a `mu` over the channel endpoint, `send(c, v)` activates its
dual. Concurrency therefore stays inside the same symmetric calculus rather
than becoming a separate runtime feature.

## 8. Runtime model

The abstract machine is a multiset of active interactions:

```text
Σ, t1 ⋈ t2 → Σ', t1' ⋈ t2' | Σ'', t_new ⋈ t_new'
```

Implementation layers:

1. **Frontend** — Rust-like parser, type checker.
2. **Core IR** — symmetric lambda calculus with explicit duals, tensors, pars,
   additives.
3. **Interaction net compiler** — translate IR to Lafont-style interaction
   nets.
4. **Optimal reducer** — Lamping/Gonthier-style optimal reduction for sharing
   (this is where performance research opportunities are).
5. **Scheduler** — executes independent interactions in parallel.

Determinism is preserved by:

- well-typedness preventing illegal interactions,
- a fixed priority order on rewrite rules,
- commuting conversions treated as canonical forms,
- continuation linearity restrictions.

## 9. Error handling

Error handling should use continuations directly, not wrap them in an enum
and pattern-match afterward. A fallible operation is just a command with one
success continuation and one or more error continuations:

```rust
command read_config(
    path: String,
    to ok: -Config,
    invalid: -ParseError,
    missing: -io::Error,
) {
    match fs::read(path) {
        Err(e) if e.is_not_found() => missing(e),
        Err(e)                     => missing(e),
        Ok(bytes) => match parse(bytes) {
            Ok(config)  => ok(config),
            Err(error) => invalid(error),
        },
    }
}
```

There is no `Result` value returned to the caller. The operation takes its
consumers as arguments and jumps to the one that applies. Failure paths are
first-class, not branches examined after the fact.

### The `?` sugar

The standard flow — propagate all errors to the caller's error continuation —
is sugar over the command form:

```rust
fn read_config(path: String) -> Config {
    let bytes = fs::read(path)?;
    let config = parse(bytes)?;
    config
}
```

desugars conceptually to:

```rust
command read_config(
    path: String,
    to ok: -Config,
    error: -Error,
) {
    let bytes = fs::read(path, to ok, error);
    let config = parse(bytes, to ok, error);
    ok(config)
}
```

More precisely, `?` means:

```text
e?
≡
e(to current_success, current_error)
```

That is, the current continuation is **split** into a success continuation
and an error continuation. The expression evaluates by activating the one the
callee chooses.

### Defining error types by continuations

Error unions are the dual of value unions. Instead of declaring an enum of
error payloads, declare the set of error continuations the operation owns:

```text
Command<I, (T, E1, E2)>   ≡   dual(I) ⅋ (T ⊗ dual(E1) ⊗ dual(E2))
```

Multiple error continuations are equivalent to a single `-(E1 + E2)`, but
the continuation spelling preserves causal structure: each path is a separate
port, statically known and typed, rather than one channel that must later be
discriminated.

```rust
command parse_int(
    text: String,
    to ok: -i64,
    empty: -String,
    overflow: -String,
) { ... }
```

To bridge to enum-based APIs, the two forms are interderivable:

```rust
command parse_int(..., to ok, empty, overflow) { ... }
// into:
fn parse_int(...) -> Result<i64, ParseIntError> { ... }
```

Continuation form and enum form are isomorphic, but the continuation form
skips allocation of a wrapper and dispatches directly to the consumer. When
every consumer is known statically, this is zero-cost in the intended
interaction-net encoding.

### Result compatibility

`Result<T, E>` remains available as a library type, but it is no longer the
primary mechanism for error handling. It is most useful at boundaries —
serializing errors, storing them, or interfacing with enum-style APIs — not
for control flow.

## 10. What the compiler pipeline looks like

```text
Slant source
  → parse (rustc-style diagnostics)
  → name/type resolution
  → desugar (fn, mu, match, ?, channels)
  → λ̄μμ̃ core IR
  → duality checking
  → interaction net encoding
  → optimization (sharing, net simplification)
  → runtime scheduler
```

The core calculus is λ̄μμ̃, structured by the classical sequent:

```text
Γ ⊢ t : A | Δ        // term (proof)
Γ | e : A ⊢ Δ        // co-term (refutation)
c                    // command (cut)
```

Core grammar:

```text
Terms (positive):
  x                      variable
  λx.t                   abstraction (dual-consuming)
  μα.c                   answer to a co-variable (surface: mu)
  t1 ⊗ t2                tensor pair
  inl(t) | inr(t)        additive injections
CoTerms (negative):
  α                      co-variable
  λ̄x.c                   co-abstraction (dual of fn)
  μ̃x.c                   value abstraction (dual of mu)
  e1 ⅋ e2                par
  fst | snd              projections
Commands:
  ⟨ t ∥ e ⟩              cut — the primitive interaction
  κx.t                   command abstraction (surface: command)
  k(v)                   continuation activation
```

Everything in the surface language compiles into these. The surface `fn`, `mu`,
and `command` are ergonomic spellings of the three syntactic categories:
terms, co-terms, and commands.

## 11. Example program

```rust
struct Point { x: i32, y: i32 }

fn distance(a: Point, b: Point) -> i64 {
    let dx = (a.x - b.x) as i64;
    let dy = (a.y - b.y) as i64;
    (dx * dx + dy * dy).sqrt()
}

fn main() {
    let origin = Point { x: 0, y: 0 };
    let p = Point { x: 3, y: 4 };
    println("distance = {}", distance(origin, p));
}
```

Looks completely ordinary. That's intentional: the symmetric core is an
implementation and semantic foundation, not something everyday users should
fight with. Explicit `dual` and `@` are the escape hatch for advanced control,
concurrency, and metaprogramming.

## 12. Syntactic symmetry audit

Symmetry must be checkable on the surface syntax, not only in the core IR.

| Concept | λ side (proof) | μ side (refutation) |
|---|---|---|
| Abstraction | `fn(x: +A) { t }` | `mu(k: -A) { t }` |
| Mixed binding | `command(x: +A, to k: -B)` | self-dual: parameters already carry polarity |
| Invocation | `f(a)` | `k(a)` (continuation activation) |
| Function type | `+A -> +B` | `dual(+A -> +B)` ≡ `-A ⅋ +B` |
| Channel | `send(c, v)` | `receive(c)` |
| Sum elimination | `match` | `comatch` (future work) |
| Data declaration | `struct` / `enum` | co-data (future work) |

Rules:

1. Every core construct has a surface spelling. Sugar may hide the symmetry
   for ergonomics, but must never make the dual side inexpressible.
2. `dual` maps λ-constructs to μ-constructs structurally, including on terms.
3. New surface features are reviewed against this table before acceptance.

Known gaps, deliberately deferred:

- **Copattern matching (`comatch`)** — define a value by how it is consumed,
  the dual of `match`. Likely syntax:

  ```rust
  let c = comatch {
      .next() => 1,
      .peek() => Some(1),
  };
  ```

- **Co-data types** — negative records and `coenum`, dual to `struct` and
  `enum`.

## 13. Suggested roadmap

1. **Core calculus formalization** — write the syntax, typing rules (including
   the λ/μ duality), reduction rules, prove subject reduction and confluence in
   a proof assistant.
2. **Interpreter** — tree-walking evaluator of the core IR; get the semantics
   right before optimizing.
3. **Surface language frontend** — parser, type checker, desugaring.
4. **Interaction net backend** — the real semantic payoff and differentiator.
5. **Standard library** — channels, Result, collections.
6. **Optimal reduction research** — a thesis-worthy optimization target;
   Lamping's algorithm is the baseline.
