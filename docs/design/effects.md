Part of the [language design](../../DESIGN.md).

## Effects and handlers

An `hook` names operations a computation may perform; a `do` answers
them. Performing an operation suspends the computation and passes control to
the nearest enclosing handler with a matching clause that is aware of
the operation. A handler installed in a function that takes a row
parameter is aware of the concrete effects named in that function's
type. It is not aware of an effect that arrived only through the row
parameter, inside a closure the caller passed in; the operation passes
through to the caller's handler. An operation is a demand, so its clause binds
the carried continuation the copattern way — after a colon, under any name
(`resume` by convention) — or omits it, for a clause that never resumes:

```sl
hook Exn    { func throw(message: String) -> i64; }
hook Reader { func config() -> i64; }
hook Choose { func flip() -> Bool; }

// `checked_div`, `scaled` and `pick` perform them, as in `examples/effects/effects.sl`.
proc main | (exit: i32) / {IO} {
    let safe = do (<(10, 0) | checked_div) hn {
        throw(message) => -1,                       // never resumes: an exception
    };
    let reading = do (<7 | scaled) hn {
        config(): resume => (<(<10 | resume, 1000) | add),  // resumes once
    };
    let all = do pick() hn {
        flip(): resume => (<(<True | resume, " ") | add | x => (x, <False | resume) | add),  // resumes twice
    };
    …
}
```

A `return(x) => e` clause maps the body's value when the body finishes
without leaving through an operation's clause. Leaving it out is the
identity: the handler's value is the body's, and its type the body's type.

Every operation clause binds exactly as many parameters as its operation
declares; a nullary operation has a clause `config()`, with no unit binder.
The handler has one answer type: the body's type without a `return` clause,
or the return clause's result type with one. Every operation clause must
produce that answer type or leave through a command. Its resumption accepts
the operation's result and returns the handler's answer, including the
return clause's transformation. Resuming a computation that never returns
does not produce an answer either.

A resumption also retains the residual effect row of its handler's body and
clauses. Passing it as a function does not make those effects pure. That row
is scoped to this installation, not all effects in the surrounding block.

An effect is handled whole: naming any of its operations requires clauses
for all of them. A partial handler instead ends with `_ => forward`.
Unmatched operations pass to an outer handler, and the effect remains in
the body's outward row; forwarding is not effect elimination. Clauses
already named still intercept their operations, including after a
resumption crosses the forwarding handler. The forwarding clause is last,
after any `return` clause, and has no binder or body. `reset` remains a
delimiter that discharges no effects.

#### Handlers as values

`hn Reader { clauses }` constructs a reusable handler value;
`hn [Reader, Other] { clauses }` lists several effects. The shorter
`hn { clauses }` infers the effects from the named operations.
`do body h` installs the handler `h` — a value or a `hand` — around `body`:

```sl
hook Reader { func config() -> i64; }
func scaled(value: i64) -> i64 / {Reader} { <(value, config()) | mul }

proc main | (exit: i32) / {IO} {
    let reader: (i64 hn String / {Reader}) = hn Reader {
        config(): resume => <10 | resume,
        return(value) => <value | to_string
    };
    <(do (<7 | scaled) reader) | println;
    <(do 42 reader) | println;
    <0 | exit>
}
```

This prints `70` and `42`. `(A hn B / {E} / {F})` is positive data: its body
produces `A`, its common answer is `B`, it discharges the concrete effects
in `E`, and its residual budget is `F`. The body's row must fit within
`E` plus `F`; installing a handler around a pure body or a subset of `E`
is valid. Clause effects must fit `F` too. All four arguments are invariant:
an annotation cannot grant a handler additional capabilities. The
clause fixes those arguments before a `let` generalizes the handler, so
a use cannot instantiate them at a type the clause has ruled out. Open handled
row tails do not grant unknown capabilities; installation subtracts only
the explicitly represented effects.

Construction installs nothing and runs no clause. Clauses run on installation
and operation demand; their effects therefore remain in the handler's type
when it is stored or returned. A `return` clause transforms even a pure body;
without one `A` and `B` coincide. Clause arity, common-answer typing and
whole-effect coverage are the same as for inline `do`. An explicit
forwarding handler retains its forwarded effects in `F` rather than claiming
to discharge them in `E`. Reusing a handler does not cache bodies or answers.
Handlers can be stored in lists, selected at runtime and composed by nesting
installations; `examples/effects/handler_values.sl` demonstrates all three. Inline
`do body hn { clauses }` uses the same clause-tree installation mechanism.

An operation is a free function, the dynamic mirror of a trait method: a
trait is an operation table keyed by a *type* and resolved statically — the
dictionary travels with the value — while an effect is an operation table
keyed by the *stack* and resolved dynamically: a handler is installed by
`do`, and its clauses bind the captured continuation, which no trait
has. That mirror
is static-versus-dynamic provisioning; the *polarity* dual of effects is a
different axis — latency, below — and the two cross: `impl Trait for Menu`
is the static column's negative row, latent rows the dynamic column's.

A function declares the effects it may perform in an **effect row** on its
arrow — `func scaled(x: +i64) -> i64 / {Reader}` — and a bare arrow is the
empty row, a pure function: the signature tells the whole truth, and every
declaration is checked locally against its own row. **Row polymorphism is
written the way the rest of the language writes generics, explicitly**: a
row variable is declared as a generic parameter and used with the `..`
"rest" spelling, and a parameter's arrow type carries the row calling it
may incur —

```sl
func map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
```

A call instantiates the callee's row variables from the arguments standing
at the positions that mention them: `<(half, xs) | map` sets `E` to `half`'s
row, so the call incurs exactly what `half` performs. `{Exn, ..E}` extends
a variable — what the written part covers does not flow through it. A
rowless arrow in a parameter's type is a promise of purity, enforced at
the call site: passing `risky` where `(i64 -> i64)` is declared is an
error at the argument. A handler discharges the effects of the operations
it answers, and `main`'s row is `{IO}` or empty, so a well-typed program
performs no operation the runtime cannot answer.

A returning stage charges its application row, with row variables
instantiated from the values reaching that stage. Feeding the closing
consumer charges its activation row too; `>` marks a non-returning transfer,
not an exemption from effect accounting.

#### Generic effects

An effect can declare type and row parameters using the same signs as other
generic declarations. Its operations share those parameters:

```sl
hook Reader<+T> { func read() -> T; }
func get<+T>() -> T / {Reader<T>} { read() }
```

`{Reader<i64>}` and `{Reader<String>}` are distinct effect applications.
Arguments are inferred from operation parameters/results, declared rows and
handler clauses; they are invariant, including rows nested inside arguments.
Names, arity, parameter kinds and polarity are checked even on unused
declarations. All effect arguments must be supplied in a written application;
an unsigned parameter takes a row such as `{IO}` or `..E`. Effect parameters
do not currently accept trait bounds.

One handler installation gives every operation of an effect the same type
arguments. Different installations may use different arguments. Runtime
dispatch remains by operation name, so a same-name operation with incompatible
arguments is rejected at the intercepting handler even if an outer handler
or residual row could otherwise accept it. Partial handlers retain this
consistency requirement and forward the typed effect outward.

A literal `hn Reader { clauses }` infers its arguments; an annotation
such as `(i64 hn i64 / {Reader<i64>})` constrains them. Merely changing
that annotation cannot change its capabilities. `examples/effects/generic_effects.sl`
demonstrates independent instantiations and stored handlers. The implementation
and additional checks are described in `docs/design-notes/generic-effects.md`.

#### `IO`: the effect the runtime handles

Reaching outside the program is an effect like any other, declared in the
prelude:

```sl
hook IO {
    func write(text: String) -> (,);
    func write_line(text: String) -> (,);
}
```

`println` and `print` are the friendly front — prelude functions over
`<T: Display>` that render their argument with `fmt`, then perform
`write_line`/`write` with the text — so a function that prints says so in
its row, and the row travels up the call graph until something handles it.
A string prints as itself, unquoted, and a value prints only if its type has
`Display`. What is special about `IO` is only where it ends: the runtime
installs a handler around `main`, so `main` may declare `{IO}` and leave it
undischarged. Nothing else may.

A handler the program installs sits nearer the operation than the
runtime's, and answers first, which is how a program mocks its own output;
a clause runs *below* its own prompt, so what the clause itself performs
escapes outward to the next handler — the runtime's — and a tap can both
report the write and forward it. `examples/effects/io.sl` writes all three.

The file operations are an effect of their own, the `fs` module's `Fs`, and
can be mocked the way `println` can. `fs::read`, `write`, `open`,
`read_line`, `close` and `exists` perform its operations, and nothing
answers them unless a program installs a handler around the code that
touches files: `fs::real`, which answers from the disk and performs `IO`, or
one of the program's own. A hand is installed around the computation —

```sl
func canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {fs::Fs, ..E} {
    do <(,) | program hn {
        fs::read_file(path): resume => <::0("canned") | resume,
        _ => forward,
    }
}
```

— and `fs` exports `real` as a hand, so a test installs its own. This
partial mock still requires an outer handler such as `fs::real` for `Fs`;
a self-contained mock answers all six operations and may discharge it. A
clause names its operation as a row names its effect, by path. An
operation answers with its outcome as a sum, `read_file(path) -> (String |
String)`, which the command then offers to its continuations: a clause runs
below its handler, so the continuations are activated by the command, under
the handler, not by the clause (`docs/design-notes/file-system-hook.md`).

`fs::real` is one hand for a value and for a command. A program that leaves
through `exit` has type `(;)`, and `do` runs that body as a command:

```sl
proc main | (exit: i32) / {IO} {
    let complain = mu String { message => { <message | println; <1 | exit> } };
    do {
        <"input.txt" | fs::read | (mu String { text => { <text | print; <0 | exit> } } & complain)>
    } fs::real
}
```

A `do` whose body is `(;)` runs it as a command under the handler. The
hand's answer is the body's, so the same clauses serve both.

The words after the program file, and a monotonic clock, are effects of
the same shape. `args::arguments` performs `Args` and answers
`List<String>`: the words after the source file, in order, not the runtime,
the path, or a flag that preceded the file. `clock::now` performs `Clock`
and answers an `i64` count of monotonic nanoseconds since an arbitrary
origin local to the process. `args::real` and `clock::real` are hands that
perform `IO`, as `fs::real` does, so `main` still leaves only `{IO}`:

```sl
proc main | (exit: i32) / {IO} {
    let words = do (<(,) | args::arguments) args::real;
    let start = do (<(,) | clock::now) clock::real;
    …
}
```

`slc run [--fuel N] [--interpret] <file.sl> [arg]…` recognizes flags only
before the file. A word after the file is a program argument, including one
that looks like a flag.

**Latent rows describe effects at activation.** A function's row is charged
when it is applied; a menu's when an item is demanded; a form's or consumer's
when it is fed. Each uses the same effect-accounting rule, with a different
activation point.

`(-> T / E)` adds a separate forcing row before that activation. A cut
that only forwards an effectful function or menu as an answer preserves
its activation row; it does not perform it. In particular, the dual of a
rowed result retains the row as a requirement on the answer, not as an
effect of forwarding that answer. Double duality remains the identity.

A menu's demand row belongs to its type:

```sl
menu Fallible / {Exn} { value: i64, doubled: i64 }
```

The arms of a `mu` over a rowed menu (and of a `mu` over a rowed form)
are checked against the declaration's latent row and charged to no
function; every demand `f.value`, and every feed of a rowed form's record,
incurs the row — so the handler that discharges it is the one around the
*demand*, and one value can answer different demands under different
handlers. A consumer type carries latency the same way: `-> (-A / {..E})`
says the returned consumer performs `..E` when *fed*, not that the call
performs anything — a returned `func`/`mu` literal is checked against
that latent row, the cut it is eventually fed at incurs it, and a `do`
around the mere construction discharges nothing, because nothing fired.
A menu or form declaration may also take a **row parameter**, declared
without a sign beside its type parameters and named by its own row:
`menu Seq<+T, E> / {..E} { next: Step<T, ..E> }`. Each use gives the row as
an argument — `Seq<i64, ..E>`, `Seq<i64, {IO}>` — and a row argument left
out at the end is the empty row, so `Seq<i64>` performs nothing. Building
one performs nothing either; a demand incurs that use's row, and a `mu`
over it checks its arms against it. A value whose demands perform less fits
where more is allowed, so a row argument is fitted one way rather than made
equal — the way its position asks. On the value, `Seq<T, ..E>` itself, the
value's row fits inside the slot's; where the position is what *takes* the
value — a function's parameter, `(Seq<T, {Tick}> -> i64)`, or the demand
a menu's bare name is — the slot's row fits inside the value's, since the
function must accept everything the slot could be given. A row parameter's
argument is a row, `..E` or `{IO}`; a type there is refused, naming the
parameter. A row variable in a declaration's row that is not one of its row
parameters is refused.

**Rows are part of types.** A row rides on the type of the value that
performs it when run — a function, a consumer, a menu or form, a delayed
computation — so it follows the value wherever the value goes, and is
performed wherever the value runs (`docs/design-notes/rows-in-types.md`).
A lambda performs its body's effects where it is called; a function bound
again by `let`, or a global handed on as a value, keeps its row; a delayed
computation stored in a tuple carries its row until it runs. Where a value
meets a slot, its row must fit inside the slot's: a pure function fits
where `/ {Exn}` is allowed, and a function that performs `Exn` does not fit
a parameter declared as a pure arrow. Command exits obey the same rule,
item by item through a bundle. A rowless exit slot promises purity;
an effectful exit keeps its row when bound, stored or forwarded, and is
charged when activated, not merely when passed. Commands that activate
arbitrary exits use explicit row parameters, for example
`proc send<E>(value: i64) | (out: (-i64 / {..E})) / {..E} { <value | out> }`.
An unused exit need not contribute to the command's own row. Builtin
commands similarly carry their possible exits' rows in their signatures.
A declaration that
hands back a value must preserve its latent row in the promised type. A
rowless returned consumer, menu or form must perform nothing when activated;
declaring its effects on the constructor cannot account for later demands.

An operation may take several parameters; since calls are curried, the
performing value collects them all before suspending. **Operations are
positive, and need no negative form.** An operation that consumes rather
than answers is already writable: `A → ⊥` *is* `-A`, so
`func drop(x: +i64) -> (;);` declares a consumer, and the cut `<42 | drop`
performs it. Routing to a chosen outcome needs nothing new
either, now that consumers are values — an operation takes them as
ordinary parameters, and the clause cuts into whichever it picks:

```sl
hook Judge { func judge(n: i64, ok: -String, bad: -String) -> (;); }
…
judge(n, ok, bad) => of (<(n, 3) | gt) { True => <"big" | ok>, False => <"small" | bad> },
```

The clause runs below its handler, on the frames the continuations it is
handed were captured on, so its cut is an ordinary jump
([§6](control.md#6-mu-capturing-the-current-continuation)).

Demand-time effects are the latent rows above. Between the three, a
`<- T` operation form would add spelling, not power, so the grammar keeps
operations to `-> T`.

A clause may resume any number of times — the continuation is a first-class
value sliced from the one frame stack. Resuming pushes that slice back onto
the running stack, above the clause's own pending work: what the resumed
computation performs reaches every handler the program has, a continuation
it captures is the whole rest of the program, and its result flows on into
the clause. Not resuming is an exception; resuming
once (with work after it, which composes) is a reader or state; resuming
twice is nondeterminism, the same captured continuation run with two answers.
Operation names are unique across effects.

Bounds and effect rows are independent of a function's polarity: a negative
function carries them in the same places — `func emit<+T: Show>(out: -String)
<- i64 / {Log}` — because a bound constrains a type parameter and a row
describes what the body performs, neither of which depends on whether the
function returns a value or a consumer.

A bound on a consumer transformer is discharged by the **cut**, not by an
argument: in `func emit<+T: Display>(out: -String) <- T`, nothing the call
receives mentions `T`, and `<42 | emit | s>` is what fixes it. So dictionary
solving waits until a declaration's body is fully checked — by then every
cut has spoken — and the same deferral gives a trait a second method
shape:

```sl
spec Describe { func describe(self: Self) -> String; }   // receives Self
spec Deliver  { func deliver(out: String) <- Self; }     // consumes Self
```

A returning method takes `self: Self` and dispatches on what it receives.
A consumer-transformer method takes no `self` — a consumer transformer's parameters are
all continuations — so its `Self` is the type it *consumes*, and dispatch
reads the value the cut sends: `<42 | deliver | s>` finds the `i64` impl,
`<True | deliver | s>` the `Bool` one. Both shapes resolve statically, and a
bound forwards through either.
