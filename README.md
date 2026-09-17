# Slant

A Rust-flavored programming language whose core semantics follow the classical
λ̄μμ̃ calculus.

## Building

```sh
cargo build
```

## Testing

```sh
cargo test
```

## Formatting

```sh
cargo run -- fmt examples/hello.sl           # rewrite in place
cargo run -- fmt --check examples/*.sl       # write nothing; fail if any file would change
cargo run -- fmt --stdout examples/hello.sl  # print the result, for an editor
```

`slc fmt` gives Slant source one layout: four-space indentation, lines within
100 columns, `;` after every statement but a block's last expression, `,`
after every arm. A list between braces keeps the line break its author gave
it, so a `match` written on one line stays there while it fits; anything too
long breaks one element per line, and a chain breaks before each `|`.
Comments and blank lines are kept. It only ever changes whitespace and the
separators the grammar leaves optional, and it refuses to write a file unless
its output parses to the same program.

## Documentation

- [examples/README.md](examples/README.md) — runnable feature examples and expected output
- `DESIGN.md` — language design
- `docs/MIGRATION.md` — syntax migration from the pre-redesign language
- `PLAN.md` — known limits, deferrals, and what is planned next
- `docs/HISTORY.md` — what the λ̄μμ̃ redesign settled
