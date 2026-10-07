# Getting started

SLC is a Cargo workspace. Building it produces the `slc` binary.

```sh
git clone https://github.com/KeenS/slc.git
cd slc
cargo build
```

The binary is `target/debug/slc`. A release build is `cargo build --release`,
and the binary is then `target/release/slc`. Either one is what the rest of
this book means by `slc`. `cargo run --` from the repository root runs the
debug binary and passes the words after `--` to it.

## Run, compile, check, and format

```sh
slc run examples/basics/hello.sl
slc check examples/basics/hello.sl
slc fmt examples/basics/hello.sl
```

`slc run` checks the program and then runs it. By default it compiles to a
native x86-64 ELF. On any other host it uses the interpreter.
`slc run --interpret` evaluates the same program on the abstract machine.
Flags belong before the file name. Words after the file are the program's
own arguments.

`slc compile` writes that executable and does not run it. `-o` names the
file. With no `-o`, the name is the source stem in the current directory.
`--fuel N` is the executable's step bound. The words passed to the executable
are the program's arguments.

```sh
slc compile -o hello examples/basics/hello.sl
./hello
```

`slc check` reports what `run` would report before evaluating, and evaluates
nothing. A file with no `main` checks as a library. The command exits
non-zero when a file fails.

`slc fmt` rewrites a file into the one layout: four-space indentation, lines
within 100 columns, a semicolon after every statement except a block's last
expression, and a comma after every `of` or `mu` arm. `--check` fails when a
file would change and writes nothing. `--stdout` prints the result.

```sh
slc fmt --check examples/basics/*.sl
slc fmt --stdout examples/basics/hello.sl
```

`slc run --fuel N` stops after N steps. With no `--fuel`, a run is bounded by
memory. `--fuel 0` runs no SLC code.

The version line is `slc --version`.

## What a program is

A program is one or more `.sl` files. The root file declares `main`. The
prelude is in every program: `Bool`, arithmetic, `println`, and the `IO`
effect. The standard library is reached by a path, such as `list::length`, or
brought in with `cite`.

The next chapter is the smallest program that prints a line and exits.
