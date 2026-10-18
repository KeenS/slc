# Parallel demands

Status: accepted. The rule is in `docs/design/effects.md` and
`docs/design/library.md`. `PLAN.md` does not carry a second copy. This
note is the reason.

## Decision

Overlap is a handler for a library effect, `Fork`, in a module named
`fork`. The operation receives two pure thunks, `lazy::Lazy` values whose
demand row is empty, and resumes once with the positional pair of their
forces. The hand `fork::sequential` forces the left thunk, then the right.
The hand `fork::parallel` denotes the same pair and forces both thunks
at once. On the empty row the two hands produce the same pair. The
forces share one fuel budget. A loop in one force does not hide a fatal
in the other. When both forces fail, which fatal is reported is not
specified.

A program that never installs either hand does not overlap. `Fork` is not
answered by the runtime around `main`.

That pair is each arm's own result. It is not a message from one arm to the
other. The message is a `Link`. Each party is an `End` menu the program
names: `writer.give(done)` sends to that end, and `writer.take` receives the
next value sent there. One function, `talk`, starts the arms. One hand,
`exchange`, matches them. A shape such as two parties or N senders is which
ends the arms hold, not another starter and not another hand. The pure hands
do not match `Link`.

## Why a handler

A clause may already resume any number of times. Resuming twice is
nondeterminism: the same captured continuation, run with two answers, one
after the other on one stack. The `flip` clause in `docs/design/effects.md`
is stricter than that. Its second cut is built from the first answer, and
`examples/effects/effects.sl` prints `HH HT TH TT` because the left
resumption finishes before the right one starts. That order is part of the
clause. A scheduler cannot overlap those two resumptions without rewriting
the clause, because the second cut does not exist until the first answer
has come back.

`Fork` is the other shape. The caller has already built two thunks. Neither
force is an input to the other. The handler demands both and resumes once
with the pair. Existing multi-shot clauses stay sequential, on one stack.

The thunk is `Lazy<T>`, not a delayed `let-`. A positive result has no
implicit delay: `let-` refuses a positive type, and `(-> T)` normalizes to
`T` (`docs/design/polarity.md`, `docs/design-notes/forcing-and-lazy.md`).
`Lazy<T>` is the explicit by-name menu, and `Lazy<T>` with the row omitted
has an empty demand row (`docs/design/data.md`). Forcing it performs
nothing. Each force runs the arm afresh. `Fork` forces each thunk once per
`both`.

```sl
pub hook Fork<+A, +B> {
    func together(left: Lazy<A>, right: Lazy<B>) -> (A, B);
}

pub func both<+A, +B>(left: Lazy<A>, right: Lazy<B>) -> (A, B) / {Fork<A, B>} {
    <(left, right) | together
}

pub hand sequential {
    together(left, right): resume => {
        let+ a = left.force;
        let+ b = right.force;
        <(a, b) | resume
    },
}
```

```sl
pub hand parallel {
    together(left, right): resume => <(<(left, right) | __fork_both) | resume,
}
```

`sequential` is ordinary SLC. `parallel` is the same operation and the same
answer. Its clause asks the runtime to force both thunks at once. One
installation of `Fork<A, B>` has one `A` and one `B`, the rule generic
effects already have for `Shift`. A region whose calls disagree on those
arguments needs a `do` per instantiation, as two shifts of different types
need two resets. A call can carry its own installation:

```sl
let pair = do (<(left, right) | fork::both) fork::parallel;
```

Nested overlap is an inner `do` inside a `force` arm. The arm of a
`Lazy<T>` returns `T` and performs nothing, so the inner `do` discharges
`Fork` before the outer thunk is pure again.

## Duplicable and affine

The empty row is duplicable. Two forces share the persistent heap and
nothing else. Lists, maps, and sets are persistent, so the two arms cannot
race on a mutable SLC cell: the language has none.

These effects are affine. A `Lazy` whose demand row mentions one of them is
not an argument of `together`:

- `IO`, `Fs`, `Env`, `Sleep`, `Process` — a single outside resource
- `Clock` — two readings of it depend on which force runs first
- `Shift` — a captured resumption re-enters the handler

`Args` is the one world effect this note marks duplicable. The words after
the program file are fixed for the run, and two forces read the same list.
That widening is not in the signature above. `together` takes `Lazy<A>`,
whose demand row is empty, so an arm that performs `Args` is already
refused. Marking `Args` duplicable, and admitting it on `together`, can
follow without a new effect.

A program-defined hook is affine. Handling it inside the `force` arm
discharges it before `both` sees the thunk, which is how a branch may still
use `Choose`, `Fs`, or `IO`: the handler sits inside the arm, and the arm
that `together` sees is pure. An ordinary shared handler is still refused.
Two forces performing one clause would race on arrival order. `Link` is the
exception, and only under `exchange`, which suspends one arm until the other
performs the complement. A matched value does not depend on which arm
arrives first.

A later note may add a four-letter mark, `copy hook`, for an effect whose
every hand is safe under a second force, and may widen `together` to a
duplicable row. The mark is a promise about every hand, including hands in
other modules. Until that note, the empty row is the whole duplicable
judgment, and the checker enforces it by the type `Lazy<A>`.

## What this does not redefine

The joint stays one consumer. `(k1 ; k2)` delivers a product left first, and
a consumer that does not return never reaches the right component
(`docs/design/data.md`). Constructing a joint does not promise independent
progress. Overlap is not a new evaluation rule for `;`.

The session reading of that connective — two protocols side by side — is not
how `Link` is spelled. `Link` is the rendezvous those papers describe, written
as two operations and one hand, not as a new meaning of `;`.

`process::spawn` stays the isolated child. The child inherits the
environment and the standard streams and shares neither a continuation nor
the SLC heap. `Fork` is the other arrangement: one program, two demands,
persistent values, one pair.

A future would fill one shared cell and return the same answer to every
later demand. Delayed computation runs afresh under the handlers around
each demand, and resumptions never memoize
(`PLAN.md`, `docs/design/control.md`). A parallel force runs the arm. It
does not store the result. Call-by-need stays out.

Copying one continuation onto two stacks is not the operation. That would
be a parallel `flip`, and it is the configuration the value restriction
exists to keep monomorphic (`docs/design/data.md`, the Harper–Lillibridge
counterexample). `together` demands two thunks the caller built. It resumes
its own continuation once.

## Communication

There is one starter and one hand. `talk` takes an `Arms` spine. `exchange`
answers that spine and answers `Link`. Two parties are a spine of two. N
senders and one receiver are a longer spine. The program holds an `End` for
each party. It does not pick a different function per shape, and it does not
write a list index at the send or the receive.

```sl
menu End<+T> / {Link<T>} {
    give(value: T): (,) / {Link<T>},
    take: T,
}
```

`writer.give(done)` is an ordinary menu demand. It suspends until the arm
that owns `writer` performs `writer.take`. `computer.take` suspends until
some arm performs `computer.give`. The hand holds the waiting resumption
and, when the giver arrived first, the value. The program names no cell, and
nothing is memoized: the next `take` does not see the previous value again.

Several arms may `give` to one end. `computer.take` still does not take
whichever giver arrived last. The givers are served in the order of the arms
in the `Arms` spine. A giver earlier in that spine is waited for even when a
later giver is already blocked on `give`. A second `give` from the same arm
is delivered at that end's next `take` after the earlier arms have been
served. One end has one taker, the arm that takes on it. A second arm
already waiting on that `take` is the fatal `end already taken`.

`end` performs `Link` and is answered by the same `exchange`, so the names
are bound inside the `do`. `mint` resumes with a token. `end` builds the
menu around that token in SLC, and the later `give` and `take` demands
perform `give_to` and `take_from` on it. The runtime does not allocate the
menu. The arm that gives does not mention the token.

Bind `end()` with `let+`. A plain `let` of the negative menu delays it, and
a delayed `end()` mints a new token on every force. A call that returns an
arm menu is delayed when it stands in the spine, because that position is
by-name and the menu is negative. Bind each arm with `let+` and place the
name in `Arm`, so the spine holds the menu.

`fork::sequential` cannot run this: the first `give` would wait for an arm
that has not started. The interpreter interleaves the arms, running one
until `give` or `take`, then another, and matching an end with the giver
the spine order names. Host threads may block in the match. The values
follow that order, so the list of results is determined.

```sl
pub hook Link<+T> {
    func mint() -> i64;
    func give_to(token: i64, value: T) -> (,);
    func take_from(token: i64) -> T;
}

pub func end<+T>() -> End<T> / {Link<T>} {
    let token = mint();
    mu End<T> {
        give(value): done <= <(token, value) | give_to | done>,
        take <= <token | take_from | take>,
    }
}

pub enum Arms<-A> {
    Stop,
    Arm(A, Arms<A>),
}

pub hook Talk<+A, +T> {
    func converse(arms: Arms<Lazy<A, {Link<T>}>>) -> List<A>;
}

pub func talk<+A, +T>(arms: Arms<Lazy<A, {Link<T>}>>) -> List<A> / {Talk<A, T>} {
    <arms | converse
}

pub hand exchange {
    mint(): resume => <(<(,) | __link_mint) | resume,
    converse(arms): resume => <(arms, resume) | __link_run,
    give_to(token, value): resume => <(token, value, resume) | __link_give,
    take_from(token): resume => <(token, resume) | __link_take,
}
```

A menu is negative, and `List<+T>` holds positive values, so the arms are
their own spine. `fork` cites `list` for the results. Every arm returns the
same `A`, and `talk` returns those results in spine order. `__link_run`
places every force above this same
hand, so a later `give` or `take` is an operation of this installation. A
clause that merely forced the thunks would sit below the hand, and the
messages would escape it (`docs/design/effects.md`). `converse` resumes
once, with the list, after every force has produced a value.

One `exchange` has one `T`. A nested `converse` while it is open is the
fatal `link already open`. When every running arm is suspended and no
`give` complements a waiting `take`, or an arm has finished while another
is still waiting on it, the program ends with the fatal `unmatched link`.
It does not spin, and the survivor does not receive a failure value.

```sl
do {
    let+ hear = end();
    let+ tell = mu Lazy<i64, {Link<i64>}> {
        force <= { hear.give(1); hear.give(2); <2 | force> },
    };
    let+ heard = mu Lazy<i64, {Link<i64>}> {
        force <= {
            let a = hear.take;
            let b = hear.take;
            <(<(a, b) | add) | force>
        },
    };
    <Arm(tell, Arm(heard, Stop)) | talk
} exchange
```

The result is `[2, 3]`.

`hear.give(1)` sends to that end. `hear.take` is how its owner receives.
The `3` is `1` and `2`. A reply is the other end: the hearer gives on
`tell`'s menu, and the teller takes on it.

N senders are the same `talk` and the same `exchange`. Each sender closes
over `total`'s end. `total` takes on its own menu three times.

```sl
// let+ total = end();
// sender: total.give(n), then return n, bound with let+
// total's arm: let a = total.take; let b = total.take; let c = total.take
// spine: Arm(sender(1), Arm(sender(2), Arm(sender(39), Arm(total, Stop))))
// do (<spine | talk) exchange   =>  [1, 2, 39, 42]
```

The three values arrive as `1`, then `2`, then `39`, because that is the
order of the sender arms in the spine. `total.take` does not name them. A
fourth `take` after those arms have finished is `unmatched link`. One
sender giving twice is two takes on `total`, not a second starter.

`Link` is affine. It is not a `copy hook`, and a pure `together` still
rejects a `Lazy` whose row mentions it. `IO`, `Fs`, and the other affine
effects stay inside an arm's private handler, or they stay out of the arm.
They are not messages on the link.

## Runtime

`sequential` is ordinary SLC. It forces the left thunk, then the right,
and resumes once with the pair. `parallel` calls `__fork_both`. The
runtime forces the two thunks at the same time and the clause resumes
with the pair it returns.

Both engines use two host threads. Each force has its own stack. They
share one fuel budget. Neither force charges that budget until both have
entered, and a force that gets ahead of its sibling yields until the
sibling has caught up or finished. After that, the two forces run
together. A step on either force counts. The pair is returned only when
both forces produce values. A fatal in either force ends the program and
stops the sibling at its next step. When both forces fail, which fatal
is reported is not specified. A successful pair does not depend on the
schedule.

The interpreter's threads run the chunk machine. The native runtime gives
each force a private stack segment. The allocator admits both threads.
The collector does not run while the forces are in progress. The pair is
allocated before the region ends, and a later safepoint may collect. Affine
runtime tables — the directory listing, the spawn buffer, the environment —
stay unreachable from that region, because the effects that touch them
cannot appear in the thunk. No SLC-level mutable cell is added to make
the join.

A pure pair under `parallel` does not put `IO` on `main`. A test that the
two hands print the same pair is the semantic test
(`examples/effects/fork_pair.sl`). A test that one force's loop does not
hide the other's fatal is the overlap test: under a fuel bound,
`sequential` reports that evaluation diverged, and `parallel` reports
division by zero.

`exchange` is scheduled by the interpreter. `__link_mint`, `__link_run`,
`__link_give`, and `__link_take` are the primitives behind it.
`__link_run` places every force above the same prompt. A blocked arm
spends no fuel. Two arms suspended with no complement spend none either:
that state is the fatal `unmatched link`. The result list uses the
qualified tags `list::List::Nil` and `list::List::Cons`, which `Display`
matches.

The native runtime does not schedule an exchange. Each of those four
primitives is a noreturn call to `slc_rt_link_unavailable`, which reports
`error: type mismatch: link exchange is not implemented in the native runtime`
and stops. Default `slc run` on x86-64 Linux is that runtime, so a Link
program uses `slc run --interpret`. The pure pair does not call those
primitives and runs under either engine.

## Landing

`docs/design/effects.md` states both rules. A `Fork` hand forces two
empty-row thunks. `sequential` does it left then right, and `parallel`
does it at the same time. `exchange` matches `writer.give(value)` with
`writer.take` among the arms of one `talk`.
`docs/design/library.md` describes the `fork` module: `both`,
`sequential`, `parallel`, `end`, `talk`, `exchange`, and `Arms`. The
examples that fix the meaning are a pure pair under `sequential` and
`parallel`, with the same printed pair
(`examples/effects/fork_pair.sl`); a thunk whose demand row is not empty,
refused at `together`; `tell` / `hear` under `exchange`, printing
`[2, 3]`; and three senders into `total`, printing `[1, 2, 39, 42]`.
The Link examples run on the interpreter. This note stays the reason.

## References

- `docs/design/effects.md`, multi-shot clauses and the `flip` handler
- `docs/design/data.md`, the joint, `Lazy<T>`, the value restriction
- `docs/design/polarity.md`, `let-`, `let+`, and by-name positions
- `docs/design/control.md`, multi-shot `Shift` without memoization
- `docs/design-notes/forcing-and-lazy.md`, `(-> T)` and `Lazy`
- `docs/design-notes/generic-effects.md`, one instantiation per installation
- `PLAN.md`, call-by-name, and call-by-need kept out
- `examples/effects/effects.sl`, sequential double resume
- Stephen Dolan, Leo White, KC Sivaramakrishnan, Jeremy Yallop, and Anil
  Madhavapeddy, "Effective Concurrency through Algebraic Effects", OCaml
  Workshop 2015. One-shot fibers. `Fork` overlaps two thunks and resumes
  once.
- Daan Leijen, "Structured Asynchrony with Algebraic Effects", TyDe 2017.
  The schedule is a handler.
- Danel Ahman and Andrej Bauer, "Runners in Action", ESOP 2019. An outside
  resource is linear, and a runner mediates it. That is the affine list
  above.
- Sam Lindley, Conor McBride, and Craig McLaughlin, "Do Be Do Be Do", POPL
  2017. Multi-shot handlers as the ordinary case, which SLC already has.
- Robert Harper and Mark Lillibridge, "Polymorphic Type Assignment and CPS
  Conversion", LICS 1993. Why a copied continuation stays monomorphic.
- Robert H. Halstead, "Multilisp: A Language for Concurrent Symbolic
  Computation", TOPLAS 1985. Futures. A future memoizes; a `Lazy` force
  does not.
- Jean-Yves Girard, "Linear Logic", Theoretical Computer Science 50, 1987,
  and Philip Wadler, "Propositions as Sessions", Journal of Functional
  Programming 24(2–3), 2014. A send on one end is a receive on the other.
  `Link` is that rendezvous. The joint is not the spelling of it.
- C. A. R. Hoare, "Communicating Sequential Processes", Communications of
  the ACM 21(8), 1978. Synchronous matching, and deadlock when neither
  operation complements the other.
