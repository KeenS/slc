# Contributing to SLC

## Getting started

1. `cargo build`
2. `cargo test`

## Workflow

1. Design changes go in `docs/design-notes/` first, then the part of the
   design they belong to. `DESIGN.md` indexes those parts.
2. `PLAN.md` holds what is open — limits, deferrals, and the `Next` queue.
   Add planned work there; when it lands, the decision goes to the design
   and the plan entry is removed rather than kept as a record.
3. Every commit should keep `cargo fmt --check`, `cargo clippy -- -D warnings`,
   and `cargo test` green.
4. SLC source — the examples, the prelude, `stdlib/` — is kept formatted by
   `slc fmt`, and `cargo test` fails when a file is not: run
   `cargo run -- fmt <file.sl>`.

## Style

- Rust 2024 edition.
- `rustfmt` with the project's `rustfmt.toml`.
- `slc fmt` for `.sl` files; its rules live in `crates/slc-fmt`.
- No `unsafe` without a SAFETY comment.
