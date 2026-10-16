# SLC: the standing plan

SLC's core is classical λ̄μμ̃. The redesign that established the language
is finished and recorded in `docs/HISTORY.md`; the language itself is
defined by `DESIGN.md` and the parts under `docs/design/`. This file is
neither — it holds only what is still open: the known limits.

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
