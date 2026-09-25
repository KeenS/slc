# SLC Language Design

SLC is a Rust-flavored programming language whose core semantics follow the
classical λ̄μμ̃ (lambda-bar-mu-mu-tilde) calculus. The surface language is
intentionally familiar to Rust programmers, but it does not pretend to be
literally symmetric: polarity is represented by types, arrows, and separate
binder groups rather than by inventing a second Rust-like syntax for
co-programs.

This asymmetry is intentional. The core calculus distinguishes terms,
co-terms, and commands, but a surface language with two parallel Rust-like
grammars would obscure both. SLC instead uses one familiar grammar and makes
polarity explicit through type signs and arrows.

What the one grammar does keep is the mirror. A program can be written
value-first — functions take data and give data back — or continuation-first,
where a function takes a consumer and gives a consumer back and nothing
returns at all. Each construct has its opposite: `fn f(x: +A) -> B` against
`fn f(k: -B) <- A`, `match` against `select`, a call against a cut, and a
`let` against the consumer that the rest of the program becomes.
`examples/duality/two_styles.sl` writes one program both ways.

Code in the design is of two kinds. A fragment marks what it leaves out
with `…`. A complete program declares `main` and leaves nothing out, and the
test suite compiles every one (`crates/slc-driver/tests/design_programs.rs`),
in this file and in every part below.

## 1. Design goals

1. **Rust-like surface** — familiar `fn`, `command`, `let`, `match`, braces, type
   annotations, and paths.
2. **λ̄μμ̃ core** — terms, co-terms, and cuts are the underlying semantic
   categories.
3. **Polarized types** — positive types denote values/proofs; negative types
   denote continuations/refutations.
4. **Explicit control** — a continuation is activated by a cut, `v | k`,
   which is a command and not a call.
5. **Total control** — every terminating path reaches a continuation: a
   `command` body must be `⊥`. The core is classical, so *which* continuation
   (and how many times) is up to the program.

## 2. Core model

```text
Program     ::= Command*
Command     ::= ⟨ Term ∥ CoTerm ⟩
Term        ::= value / proof
CoTerm      ::= continuation / refutation
Cut         ::= ⟨ Term ∥ CoTerm ⟩
```

Evaluation proceeds by cuts. A proof meets a refutation; the interaction
determines which reduction fires. There is no privileged application head and
no language-level concurrency primitive.

## Where the rest is

The design is this file and the parts under `docs/design/`. Section numbers
belong to the language, and each part keeps the numbers it had here.

- [Flow](docs/design/flow.md) — §3, application, composition, and the cut.
- [Polarity](docs/design/polarity.md) — §4, function orientation and evaluation.
- [Control](docs/design/control.md) — §5 `command`, §6 `mu`.
- [Data](docs/design/data.md) — §7 additive data, §8 multiplicative data,
  polymorphism, and what a type may leave unwritten.
- [Traits](docs/design/traits.md) — ad-hoc polymorphism.
- [Effects](docs/design/effects.md) — effects and handlers.
- [Programs](docs/design/programs.md) — §9 the entry point, literals,
  diagnostics, and §10 modules.
- [Library](docs/design/library.md) — the prelude and the standard library.
- [Core](docs/design/core.md) — §11 the core calculus, §12 error continuations.
