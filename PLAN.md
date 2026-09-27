# SLC: the standing plan

SLC is a Rust-flavoured surface over a classical λ̄μμ̃ core. The redesign
that established it is finished and recorded in `docs/HISTORY.md`; the
language itself is defined by `DESIGN.md` and the parts under
`docs/design/`. This file is neither — it holds
only what is still open: the known limits, the work queued next, and what is
deferred for discussion.

The contract that keeps it short: a decision, once made, goes to the design
(`DESIGN.md` indexes the parts) and leaves this file. The plan is where work
stops being open, not where it is remembered — so an entry here is a promise
still outstanding, and nothing else belongs. Entries refer to each other by
name, never by position.

## Status

The acceptance suite covers the corrected runtime and effect rules, and the
complete design programs. The queue the redesign approved is complete, and
the known limits below remain.

Delayed computation is call-by-name: each demand runs afresh under the
handlers around that demand. Later work keeps the polarity-directed
evaluation discipline and explicit eagerness, the separate forcing and
activation rows, and handler boundaries at demand time. Call-by-need,
memoized thunks, and implicit result caching stay out of this file, as does
a general call-by-value default.

## Known limits

### Of the design

- **A function keeps the orientation it was written with.** `A -> B` and
  `B <- A` are different types. A chain reads a stage in the orientation
  that stage has. A value of one is not accepted where the other is declared.

- **Stored handlers discharge concrete capabilities.** `(A hn B / {E} / {F})`
  preserves both rows, but installation discharges only effects explicitly
  present in `E`; an unknown handled-row tail does not grant capabilities.
  Abstraction over unknown capability tails remains unsupported; typed
  generic applications inside concrete capabilities are supported.

## Next

- **Integers and floats meet.** `Into` converts among the integer widths.
  A coordinate is computed from a pixel index, and a float result is
  reported as an integer. `benches/mandelbrot.sl` steps by the literal
  `0.0625` and counts the points that stay inside. Both directions are
  part of this work, including the rule for a value that does not fit and
  for an `i64` with no exact `f64`.

- **Real functions.** The prelude's float operations are the arithmetic,
  remainder, and comparisons. A distance needs a square root, so an
  n-body simulation stays out of `benches/` until one exists. Square root
  comes first. Its domain is part of this work. `abs`, `floor`, and
  `ceil` sit beside it. Printing a real result as an integer uses
  **Integers and floats meet**. `sin`, `ln`, and `pow` are **Further real
  functions**.

- **Generic min and max.** `num::min` and `num::max` take `i64`. `Ord`
  already compares the base types, so both become generic over `Ord`.
  `num::abs` stays on `i64`. A float absolute value is **Real functions**.

- **The list operations.** `list` exports `length`, `append`, `map`, and
  `nth`. Range, filter, fold, reverse, take, drop, and sum are rebuilt in
  the benchmark programs: the partition in `benches/quicksort.sl`, the
  reversal in `benches/fannkuch.sl`, and the candidates in
  `benches/sieve.sl`. They are eager functions on `List`. `fold` is the
  general form, and `sum` is `fold` of addition. `seq` keeps filter and
  take. `stream` keeps take and drop.

- **Program arguments.** `slc run` takes a file and an optional fuel
  bound. Benchmark sizes are `def` bindings in the source. A program
  reads the arguments it was given. The read is an effect the runtime
  answers, as `IO` and `Fs` are. The answer type is part of this work.

- **A clock.** `benches/run.sh` times the process and subtracts a separate
  `slc check`. A program reads the time and times its own work. The read
  is an effect the runtime answers, as `IO` and `Fs` are. The answer type
  is part of this work.

## Deferred, for discussion

Each of these waits until a program needs it.

- **Further real functions.** `sin`, `ln`, and `pow`. **Real functions**
  covers square root, `abs`, `floor`, and `ceil`. These three wait until
  a program computes one.

- **String scanning.** `starts_with`, and a walk over a string's scalar
  values. `find_char`, `substring`, `is_digit`, and `skip_digits` already
  cover a hand-written scanner such as
  `examples/programs/json_parser.sl`. `split`, `trim`, `replace`, and case
  conversion wait with this until a program does one of those.

- **Set algebra.** `union`, `intersection`, and `difference` on the
  ordered `set::Set`. `to_list` already lists the keys. This waits until
  a program combines two sets.

- **Directory commands.** `read_dir`, `remove`, and `rename` as `Fs`
  commands, one continuation per outcome, beside `read` and `write`.
  This waits until a program does more than read and write one file.

- **Option combinators.** `option::map` and `option::and_then`.
  `unwrap_or` is already exported. This waits until a program rebuilds
  that match.

- **Bool operations.** `and` and `or` beside `not`, strict functions of
  two `Bool`s. `examples/programs/regex_derivative.sl` writes both
  locally. This waits until a second program wants them.

- **Shifts.** `shl` and `shr` as `i64` functions beside `xor` and
  `wrapping_mul`. The hash trie selects a slot with `div` and `rem`. This
  waits until a program packs bits.

- **Environment and process.** Environment variables, sleep, and spawning
  a process. Each is an effect the runtime answers, as **Program
  arguments** and **A clock** are. Each waits until a program does that
  thing.
