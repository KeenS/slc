# Diagnostics

A compiler diagnostic names its phase, the file, and a line and column. It
quotes the source it is about. A syntax error that runs to the end of the
file, such as an unterminated string, quotes its first line. A diagnostic
with no extent names no place.

The phases run in this order, and a phase that reports anything stops the
later ones:

| Phase | What it refuses |
|---|---|
| `parse` | A token stream that is not a program, including an older keyword |
| `resolve` | A name that does not denote, or a cite that collides |
| `trait` | An impl that does not match its spec, or that overlaps another |
| `type` | A term whose type does not meet its port |
| `polarity` | A value or continuation used at the wrong polarity |
| `exhaustiveness` | An `of` or `mu` that misses a shape, repeats one, or overlaps |
| `effect` | An effect the row does not allow, or a handler that does not cover it |
| `lowering` | A surface form the core cannot express |

Within one phase, diagnostics are source-ordered. `slc check` and `slc run`
report the same compiler diagnostics. `run` continues into evaluation only
when the phases accept the program.

Some slips name the repair directly. A handler clause with the wrong number
of parameters reports the operation's arity. A clause of the wrong answer
type reports the handler's answer type. An incomplete handler lists the
missing operations and mentions a final `_ => forward`. A function closed
with `>` where the `>` was meant to be left off is reported as a function
applied by flow. A clause that names no operation is refused by that name.

Runtime failures happen after evaluation starts. They are not compiler
diagnostics and do not follow this order. They include:

- division by zero, and arithmetic overflow, including an `Into` conversion
  or a `sqrt` that does not fit
- a string index or substring outside the string
- a read through a `File` that has been closed
- a continuation that jumps across a `reset` which did not capture it

The programs in
[`examples/errors/`](https://github.com/KeenS/slc/tree/master/examples/errors)
fail on purpose. One is a command that reaches no continuation, one is a
polarity the checker refuses, and one crosses `reset` at run time.
