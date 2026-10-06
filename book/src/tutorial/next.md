# Where to go next

The tutorial covered the surface a program uses every day: chains, values,
continuations, data, both arrows, commands, traits, effects, sections, the
library, and demand.
The reference half of this book is the lookup for the same language. The
specification remains
[`DESIGN.md`](https://github.com/KeenS/slc/blob/master/DESIGN.md).

## Examples

Every file under
[`examples/`](https://github.com/KeenS/slc/tree/master/examples) runs with
`slc run`, and the test suite checks its output.
[`examples/README.md`](https://github.com/KeenS/slc/blob/master/examples/README.md)
lists them.

| Directory | What it holds |
|---|---|
| `examples/basics/` | Literals, data, traits, the library |
| `examples/duality/` | Both writing styles, menus, forms, commands |
| `examples/effects/` | Handlers, rows, delimited control |
| `examples/laziness/` | Demand, streams, sequences |
| `examples/programs/` | A JSON parser, files, a regex menu, several files |
| `examples/errors/` | Programs the checker refuses, on purpose |

`slc check examples/basics/*.sl` is a useful sweep. Leave `examples/errors/`
out of that glob.

## A longer program

[`examples/programs/json_parser.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/json_parser.sl)
parses by sending each outcome to a continuation, and it leaves a failing
check early so the rest of the function is the success path.
[`examples/programs/file_io.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/file_io.sl)
reads and writes under `fs::real`.
[`examples/programs/regex_derivative.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/regex_derivative.sl)
is a regular expression as a menu.

## Benchmarks and the editor

[`benches/`](https://github.com/KeenS/slc/tree/master/benches) holds programs
with a fixed workload. Each prints one integer. `benches/run.sh` times a
check and a run and checks that integer.

[`editors/emacs/slc-mode.el`](https://github.com/KeenS/slc/blob/master/editors/emacs/slc-mode.el)
is an Emacs major mode for `.sl` files: highlighting, comments, and
indentation that agrees with `slc fmt`.

[`PLAN.md`](https://github.com/KeenS/slc/blob/master/PLAN.md) is what the
implementation still leaves open.
[`docs/MIGRATION.md`](https://github.com/KeenS/slc/blob/master/docs/MIGRATION.md)
is the older spelling of each keyword, which the parser still names when it
sees one.
