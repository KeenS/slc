# SLC: the standing plan

SLC's core is classical λ̄μμ̃. The redesign that established the language
is finished and recorded in `docs/HISTORY.md`; the language itself is
defined by `DESIGN.md` and the parts under `docs/design/`. This file is
neither — it holds only what is still open: the known limits, and what is
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

## Deferred, for discussion

Each of these waits until a program needs it.

- **Further real functions.** `sin`, `ln`, and `pow`. Square root, `abs`,
  `floor`, and `ceil` are already in the prelude. These three wait until
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
  a process. Each is an effect the runtime answers, as program arguments
  and the monotonic clock are. Each waits until a program does that
  thing.
