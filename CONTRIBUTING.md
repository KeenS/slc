# Contributing to Slant

## Getting started

1. `cargo build`
2. `cargo test`

## Workflow

1. Design changes go in `docs/design-notes/` first, then `DESIGN.md`.
2. `PLAN.md` holds what is open — limits, deferrals, and the `Next` queue.
   Add planned work there; when it lands, the decision goes to `DESIGN.md`
   and the plan entry is removed rather than kept as a record.
3. Every commit should keep `cargo fmt --check`, `cargo clippy -- -D warnings`,
   and `cargo test` green.

## Style

- Rust 2024 edition.
- `rustfmt` with the project's `rustfmt.toml`.
- No `unsafe` without a SAFETY comment.
