# The file system as an effect

Status: implemented. `DESIGN.md`'s `IO` section and the `fs` module are the
definition; this note records why the design was taken. Landing it also
qualified an effect's operations and the effects rows name by their module,
and typed a handler's clauses from the operations they answer.

## Why

The `fs` module's commands charge `{IO}` and reach the outside world through
runtime primitives (`__read_file` and its siblings) rather than by performing
an operation, so no handler can answer them: a test cannot give a program a
file system that is not the disk. `println` has no such problem, because it
performs `IO`'s `write_line`, which a program's own handler may answer first.

## What rows in types changed

The plan assumed a reusable handler needs a first-class handler value. A
function that installs a handler around a computation it is handed was
refused while effects followed names, because the lambda's `Fs` was charged
where the lambda was written. With rows in types
(`docs/design-notes/rows-in-types.md`) the lambda carries `Fs` on its type, and
the function is accepted. This runs today:

```sl
effect Fs { fn read_file(path: String, ok: -String, failed: -String) -> (;); }

// The real file system.
fn real<+A, E>(program: ((,) -> A / {Fs, ..E})) -> A / {IO, ..E} {
    handle <(,) | program {
        read_file(path, ok, failed) => <path | __read_file | (ok & failed)>,
    }
}

// A mock: nothing touches the disk.
fn canned<+A, E>(program: ((,) -> A / {Fs, ..E})) -> A / {..E} {
    handle <(,) | program {
        read_file(path, ok, failed) => <("canned ", path) | add | ok>,
    }
}
```

`real` reads `examples/hello.sl`, routes a missing file to `failed`, and
`canned` answers without the disk; what the program performs besides `Fs`
passes through both.

## Proposal

1. **Originally no new language feature; now superseded.** The function
   encoding above remains useful, but first-class handler values are now
   implemented: `handler { … }` constructs one and `with h handle c`
   installs it. `DESIGN.md` specifies `Handler<A, B, E, F>`; the
   `examples/handler_values.sl` example stores, selects and composes them.
2. **`fs` declares `Fs`,** one operation per primitive: reading a file,
   writing one, opening one, reading a line, closing, and asking whether a
   path exists. An operation answers with its outcome as a sum, one
   alternative per continuation of the command that performs it —
   `read_file(path) -> (String | String)` — rather than taking the
   continuations as parameters. A clause runs below its handler, so a
   continuation it activated would run outside the handler, and a file
   operation performed there would find none; resuming with the outcome lets
   the command activate the continuation, under the handler. Operation names
   are unique across effects, so they are named for the file system rather
   than reusing `read` and `write`.
3. **The commands keep their shapes.** `<path | fs::read | (ok & failed)>` and its
   siblings stay, as commands that perform the operation, so a call site does
   not change; their rows say `{Fs}` instead of `{IO}`.
4. **`fs::real` is the standard handler,** answering each operation with its
   primitive and declaring `{IO, ..E}`. A program's own handler function, or
   a `handle` written around the code, answers first — which is how a test
   mocks the file system.
5. **A program installs `fs::real`** around the code that touches files — see
   "Decided".

## Consequences

A handler naming an operation of `Fs` must answer all six operations, or
end with `_ => forward`. Forwarding leaves `Fs` in the outward row and
requires an outer handler, typically `fs::real` or `fs::real_command`.
A complete mock can discharge `Fs` without touching the disk. The runtime
already routes unmatched operations outward; the explicit clause makes
that behavior visible to the checker rather than silently erasing the effect.

- A declaration that touches files says `{Fs}` in its row rather than `{IO}`,
  and something between it and `main` installs a handler for `Fs`.
- `examples/file_io.sl` is rewritten to install the handler; its output does
  not change. A new example, or a test, reads a file through a mock.
- The primitives still charge `{IO}`: only a handler calls them.
- `DESIGN.md`'s `IO` section loses its stale reason that an operation cannot
  carry an outcome, and `PLAN.md` its known limit "The file operations
  perform `IO` without an operation".

## Decided

Reviewed, with both proposals taken:

1. Handler-installing functions remain supported alongside first-class
   handler values. The later handler-value implementation supersedes the
   original decision to defer them. Yielding command exits also let
   `fs::real` and `fs::real_command` resume with outcomes without explicit
   `mu` result captures.
2. A program installs `fs::real` itself, around the code that touches files.
   `main` still leaves only `IO` undischarged, and nothing is special-cased.
