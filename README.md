# SLC

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

## Running and checking

```sh
cargo run -- run examples/basics/hello.sl     # compile and run
cargo run -- check examples/*/*.sl            # every compiler phase, and no run
```

A program may be several files: `mod geometry;` declares a module whose
body is `geometry.sl`, beside the program, and the directory tree is the
module tree (`examples/programs/multi_file/`). Run and check the root.

`slc check` reports what `run` would report before evaluating — parse, type,
polarity, exhaustiveness and lowering diagnostics, each with its file, line
and column — and exits non-zero if any file fails. A file with no `main`
checks as a library.

## Formatting

```sh
cargo run -- fmt examples/basics/hello.sl           # rewrite in place
cargo run -- fmt --check examples/*.sl       # write nothing; fail if any file would change
cargo run -- fmt --stdout examples/basics/hello.sl  # print the result, for an editor
```

`slc fmt` gives SLC source one layout: four-space indentation, lines within
100 columns, `;` after every statement but a block's last expression, `,`
after every arm. A list between braces keeps the line break its author gave
it, so a `match` written on one line stays there while it fits; anything too
long breaks one element per line, and a chain breaks before each `|`.
Comments and blank lines are kept. It only ever changes whitespace and the
separators the grammar leaves optional, and it refuses to write a file unless
its output parses to the same program.

## Editor support

[`editors/emacs/slc-mode.el`](editors/emacs/slc-mode.el) is an Emacs major
mode for `.sl` files: highlighting, comment and string syntax, imenu, and
indentation that agrees with `slc fmt`.

```elisp
(add-to-list 'load-path "/path/to/slc/editors/emacs")
(require 'slc-mode)
(setq slc-command "/path/to/slc/target/release/slc") ; if `slc` is not on PATH
(setq slc-format-on-save t)                          ; optional
```

`C-c C-f` formats the buffer through `slc fmt`. The mode's tests run with

```sh
emacs -Q --batch -L editors/emacs -l editors/emacs/slc-mode-tests.el -f ert-run-tests-batch-and-exit
```

## Documentation

- [examples/README.md](examples/README.md) — runnable feature examples and expected output
- [`DESIGN.md`](DESIGN.md) — language design; the parts are under [`docs/design/`](docs/design/)
- `docs/MIGRATION.md` — syntax migration from the pre-redesign language
- `PLAN.md` — known limits, deferrals, and what is planned next
- `docs/HISTORY.md` — what the λ̄μμ̃ redesign settled
