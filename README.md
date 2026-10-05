# SLC

SLC is a programming language. A program is a command: it sends a value to
a continuation, and that meeting is a step of the program. Its core
semantics follow the classical λ̄μμ̃ calculus (lambda-bar-mu-mu-tilde).
This repository is the compiler, type checker, formatter, and runtime,
written in Rust. Source files end in `.sl`. The command-line tool is `slc`.

There is one grammar. A type denotes a value or a continuation, and the
arrow says which way a function faces, so the same program can be written
value-first or continuation-first.
[`examples/duality/two_styles.sl`](examples/duality/two_styles.sl) writes
one program both ways. The language is specified in
[`DESIGN.md`](DESIGN.md).

## A first program

[`examples/basics/hello.sl`](examples/basics/hello.sl):

```sl
proc main | (exit: i32) / {IO} {
    <"Hello, SLC!" | println;
    <0 | exit>
}
```

`proc main` is the entry point. It takes one continuation, `exit`, and the
runtime supplies it. `<` opens a chain with a value, and `|` carries that
value from left to right. The first line sends the string to `println`.
The second sends `0` to `exit`, which ends the program; the integer is the
process exit status. `{IO}` names the effect of printing, which the runtime
handles. Every terminating path of `main` sends a status to `exit`.

```sh
cargo run -- run examples/basics/hello.sl
```

That prints:

```text
Hello, SLC!
```

and exits with status 0.

## The same computation, two ways

A function written with `->` takes data and returns data. `of` takes a
value apart: a pattern, then `=>`, then the expression that pattern
produces. `<(3, r) | mul` multiplies the pair, and `x =>` binds the
product so the chain can continue.

```sl
func area(s: Shape) -> i64 {
    of s {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul,
        Rect(w, h) => <(w, h) | mul,
    }
}
```

A function written with `<-` takes a consumer of the result and returns a
consumer of the input. `mu` takes the value apart in the same shape, and
each arm sends its result onward.

```sl
func area_of(out: i64) <- Shape {
    mu Shape {
        Circle(r) => <(3, r) | mul | x => (x, r) | mul | out>,
        Rect(w, h) => <(w, h) | mul | out>,
    }
}
```

`Shape` is an enum of `Circle` and `Rect`. `->` returns a value. `<-`
produces a consumer. A chain reads each stage in the orientation that
stage was written with, so both functions sit in the same left-to-right
pipeline. The whole program is
[`examples/duality/two_styles.sl`](examples/duality/two_styles.sl).

## What you can write

Everyday programs use functions, `let`, enums, records, tuples, and `of`.
A trait is a `spec` of operations and an `impl` for a type; calls resolve
at compile time. An effect is a named operation a computation performs. A
handler answers it, and `do` installs the handler for an expression. A
delayed computation runs afresh on every demand, under the handlers around
that demand.

The prelude is in every program: `Bool`, integer and floating-point
arithmetic, `println`, and `IO`. Lists, maps, sets, arrays, strings,
streams, and files are a standard library written in SLC. A program
reaches a module by a path (`list::length`) or brings a name in with
`cite`.

A program may be several files. `sect geometry;` declares a module whose
body is `geometry.sl`, beside the program, and the directory tree is the
module tree
([`examples/programs/multi_file/`](examples/programs/multi_file/)). Run
and check the root file.

## Where to start reading

Each example runs with `cargo run -- run <file.sl>`, and the test suite
checks its output. [`examples/README.md`](examples/README.md) lists every
file.

| Topic | Start here |
|---|---|
| Literals, functions, data, traits, modules | [`examples/basics/hello.sl`](examples/basics/hello.sl), then [`examples/basics/`](examples/basics) |
| Values and continuations, both writing styles | [`examples/duality/two_styles.sl`](examples/duality/two_styles.sl) |
| Effects and handlers | [`examples/effects/effects.sl`](examples/effects/effects.sl) |
| Evaluation on demand, streams | [`examples/laziness/`](examples/laziness) |
| Longer programs | [`examples/programs/`](examples/programs) |
| Programs the checker refuses | [`examples/errors/`](examples/errors) |

## Building, running, and checking

The repository is a Cargo workspace. `cargo build` produces `slc`.

```sh
cargo build
cargo test
cargo run -- run examples/basics/hello.sl     # check, then run
cargo run -- check examples/basics/*.sl       # the same checks, then stop
```

`slc run --fuel N` stops after N steps of the evaluator. With no
`--fuel`, a run is bounded by memory. Words after the file are the
program's arguments: `slc run [--fuel N] [--interpret] <file.sl> [arg]…`.
Flags are recognized only before the file.

`slc check` reports what `run` would report before evaluating. A
diagnostic names its phase — `parse`, `resolve`, `trait`, `type`,
`polarity`, `exhaustiveness`, `effect`, or `lowering` — and the file,
line, and column. The command exits non-zero if any file fails. A file
with no `main` checks as a library. The programs in
[`examples/errors/`](examples/errors) fail these checks on purpose.

## Benchmarks

[`benches/`](benches) holds programs with a fixed workload. Each prints one
integer. [`benches/run.sh`](benches/run.sh) times `slc check` and `slc run`
and checks that integer. The sizes and the expected integers are described
in [`benches/README.md`](benches/README.md).

## Formatting

```sh
cargo run -- fmt examples/basics/hello.sl           # rewrite in place
cargo run -- fmt --check examples/basics/*.sl       # fail if any file would change
cargo run -- fmt --stdout examples/basics/hello.sl  # print the result, for an editor
```

`slc fmt` gives SLC source one layout: four-space indentation, lines within
100 columns, `;` after every statement but a block's last expression, `,`
after every arm. A list between braces keeps the line break its author gave
it, so an `of` written on one line stays there while it fits; anything too
long breaks one element per line, and a chain breaks before each `|`.
Comments and blank lines are kept. It only ever changes whitespace and the
separators the grammar leaves optional, and it refuses to write a file unless
its output parses to the same program. `--check` leaves every file as it is.

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

- [`examples/README.md`](examples/README.md) — runnable feature examples and the output each one gives
- [`DESIGN.md`](DESIGN.md) — the language; the parts are under [`docs/design/`](docs/design/)
- [`PLAN.md`](PLAN.md) — known limits, and what is still open
- [`docs/HISTORY.md`](docs/HISTORY.md) — what the λ̄μμ̃ redesign settled
- [`docs/MIGRATION.md`](docs/MIGRATION.md) — syntax migration from the pre-redesign language
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how a change lands

## Repository

| Crate | What it holds |
|---|---|
| [`slc-syntax`](crates/slc-syntax) | Lexer, parser, name resolution, traits, lowering to the core |
| [`slc-check`](crates/slc-check) | Types, polarity, exhaustiveness, effect rows |
| [`slc-core`](crates/slc-core) | The λ̄μμ̃ core: terms, co-terms, commands, reduction |
| [`slc-runtime`](crates/slc-runtime) | The evaluator: a flat program and an abstract machine |
| [`slc-fmt`](crates/slc-fmt) | The formatter |
| [`slc-driver`](crates/slc-driver) | The `slc` binary, the prelude, and the standard library |

The prelude is [`crates/slc-driver/src/prelude.sl`](crates/slc-driver/src/prelude.sl).
The standard library is [`crates/slc-driver/src/stdlib/`](crates/slc-driver/src/stdlib).
Both are ordinary SLC and go through the same pipeline as a user program.

## License

SLC is licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
