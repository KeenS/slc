# spawn

Status: accepted. The rule is in `docs/design/effects.md`.

## Decision

`spawn` takes a proc with `main`'s signature and runs it on another host
thread. Naming that proc is `(+i32 / {IO})`: the empty value group is
applied first, then the exit. The call performs `IO` and returns `(,)`.
The `i32` that reaches the exit stays on the spawned thread.

The process waits for every such thread before it reports `main`'s
status. A fatal in one of them fails the process when `main` itself
exited 0. The proc's `i32` is not that status.

The interpreter gives the thread its own fuel copy. The native runtime
has one `slc_fuel` counter, so a diverging proc can exhaust it. The
thread that decrements the counter to zero stops.

`process::spawn` stays an isolated operating-system child. It does not
share this heap or these threads.

The program is [`examples/effects/spawn.sl`](../../examples/effects/spawn.sl).
