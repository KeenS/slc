# The slc command

```text
slc --version
slc run [--fuel N] [--interpret] <file.sl> [arg]…
slc compile [-o <file>] [--fuel N] <file.sl>
slc check <file.sl>…
slc fmt [--check | --stdout] <file.sl>…
```

`slc --version` prints `slc` and the package version.

## run

`slc run` checks the file and then runs it. Flags are recognized only before
the file. Everything after the file is a word the program reads through
`args::arguments`, including a word that looks like a flag.

With no `--interpret`, `slc` compiles to a native executable, links it with
the runtime, and executes it. `--interpret` evaluates the checked program on
the abstract machine. `book/check.sh` runs the tutorial programs with
`--interpret`.

`--fuel N` stops after N steps. Omitting `--fuel` bounds a run by memory.
`--fuel 0` runs no SLC code. The integer sent to `exit` is the process
status when it fits in a byte. A status outside `0..=255` fails the process.

A file with a `main` of the wrong shape is refused before it runs. The
required shape is a root `proc` with no value parameters and one
continuation, the status.

## compile

`slc compile` checks the file, links the native executable, and writes that
file. It does not run the program.

`-o` names the executable. `--output` is the same flag. With neither, the
name is the source stem in the current directory, so
`examples/basics/hello.sl` becomes `./hello`. The output path has to be a
different file from the source.

`--fuel N` is the executable's step bound, the same bound `slc run --fuel N`
uses. With no `--fuel`, the executable is bounded by memory. Every argument
of the executable is a program argument.

```sh
slc compile -o hello examples/basics/hello.sl
./hello
```

A file with no `main`, or a `main` of the wrong shape, is refused and no
executable is written.

## check

`slc check` applies the compiler phases and stops. It evaluates nothing, so
a program that would not terminate can still be checked. Each diagnostic is
prefixed with its file. One failing file fails the command. A file with no
`main` is a library and checks. A glob such as `examples/basics/*.sl` is the
shell's, and `examples/errors/` is left out of it because those files fail
on purpose.

## fmt

`slc fmt` rewrites each file in place. The layout is four-space indentation,
lines within 100 columns, a semicolon after every statement except a block's
last expression, and a comma after every arm. A list between braces keeps
the line break its author gave it while it fits. A chain breaks before each
`|` when it does not fit. Comments and blank lines are kept.

The formatter changes whitespace and the separators the grammar leaves
optional. It refuses to write a file whose formatted text is not the same
program. `--check` writes nothing and fails when a file would change.
`--stdout` prints the result. `--check` and `--stdout` together are refused.

## Building

From the repository:

```sh
cargo build
cargo test
cargo run -- run examples/basics/hello.sl
cargo run -- fmt --check examples/basics/*.sl
```

`cargo test` includes the example outputs, the design programs, and a check
that SLC sources are formatted. The Emacs mode is
[`editors/emacs/slc-mode.el`](https://github.com/KeenS/slc/blob/master/editors/emacs/slc-mode.el).
`C-c C-f` formats the buffer through `slc fmt`.
