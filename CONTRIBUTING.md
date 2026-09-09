# Contributing to Slant

## Getting started

1. `cargo build`
2. `cargo test`

## Workflow

1. Design changes go in `docs/design-notes/` first, then `DESIGN.md`.
2. `PLAN.md` records what the redesign settled and what it left undone; add
   planned work to its `Next` section and fold the result back in when it lands.
3. Every commit should keep `cargo fmt --check`, `cargo clippy -- -D warnings`,
   and `cargo test` green.

## Style

- Rust 2024 edition.
- `rustfmt` with the project's `rustfmt.toml`.
- No `unsafe` without a SAFETY comment.
