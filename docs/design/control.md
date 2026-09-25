Part of the [language design](../../DESIGN.md).

## 5. `command`: consumer abstraction

A `command` declaration is the form that takes **both** values and
continuations: value parameters and continuation parameters appear in separate
parenthesized groups, and the body is a command — hence the name. A
declaration that consumes values and consumes a continuation is a `command`; a
returning `fn` may still receive a consumer as a value it forwards — `-String`
is a value type like any other — but it returns rather than ending in a cut.

```sl
command route(x: i32) | (k: i32) {
    <x | k>
}
```

The declaration denotes a command. Its return type is bottom; an optional
`-> (;)` annotation may be used as documentation and does not change lowering.

A group with nothing in it is left out rather than written empty: `command
main | (exit: i32)` takes no values, and `command log(message: String)` takes
no continuations. An empty `()` is a parse error saying so.

Conceptually, `command f(x: A) | (k: B) { E }` lowers to `λx. λk. E`: the
value parameters bind first, so a call supplies arguments in the order the
parameters are written. Control leaves the body only by activating one of its
continuations. `k` is a *parameter*: the caller passes it.

### Yielding through returning exits

A command's exit group may instead be a bundle of returning functions. For
exits `(-A & -B)`, a bundle `((A -> R) & (B -> R))` lets the chain produce
the chosen function's common result `R`. Leave off the closing `>` and
continue the chain normally:

```sl
let outcome = <path | __read_file | (
    fn(text: String) { ::0(text) } & fn(reason: String) { ::1(reason) }
);
```

Every exit must return the same type; mixing returning functions and
non-returning consumers is rejected. With a closing `>`, exits remain
consumers and the chain is a command. Yielding is an adapter that captures
the result continuation and composes each callback into it, not a change to
`select`: its arms still end in commands. The selected callback's forcing
and activation effects remain under the handlers around the yielding call;
unselected callbacks are not demanded. `examples/duality/yielding_commands.sl`
compares this syntax with an explicit `mu`. The file-system handlers use
yielding exits to resume with outcomes without hand-written captures.

## 6. `mu`: capturing the current continuation

The expression `mu { k <= … }` is the other half, and it is the real μ of
the calculus: it captures the continuation of the expression it stands in —
the language's `call/cc`. Nothing supplies `k`; an expression has no caller,
only a context, and in λ̄μμ̃ a context *is* the co-term on the right of a cut:

```text
⟨ μk. c ∥ e ⟩  →  c[e/k]
```

`mu` is uniformly `mu [Type] { arms }`, mirroring `select`: one binder arm,
`k <= c`, is the atom form and captures the ambient continuation whole;
request arms, `item: out <= c`, are the copattern form and build a menu.
When the continuation binder has the item's name, `item <= c` abbreviates
`item: item <= c`; an untyped single bare arm remains the local binder form.
Every `mu` arm writes `<=`, and every `select` arm `=>` — the arrow marks
what arrives: data flows forward into an arm, a demand reaches back.
A binder arm stands alone — it takes the whole continuation, so a second
arm would have nothing left to answer. There is no value-binding `mu`
because a binder whose body is a *command* rather than an expression is
`select`, the μ̃.

So `k` is bound to whatever consumer the expression meets. The type written
in front is what the expression produces — `mu String { k <= c }` is a
`String` and `k` consumes one — and it may be left off when the arm says
it. A call whose result comes back through a continuation can be written
without nesting the rest of the program inside it:

```sl
let source = mu String { k <=
    <path | fs::read | (k & complain)>
};
<source | print;
```

`k` is the continuation of the `let`: what `fs::read` sends it becomes
`source`, and the block continues. On the other outcome `k` is never
activated, so nothing after the `let` runs.

This is how a fallible operation is written. Rather than returning a result
that a caller inspects, it takes the continuations its outcomes belong to:

```sl
command parse_value(input: String, pos: i64) | (ok: i64 & failed: String) {
    match <(input, pos) | at {
        QUOTE => <(input, pos) | parse_string | (ok & failed)>,
        _ => <"expected JSON value" | failed>,
    }
}
```

Each path ends in a cut: either forwarding both continuations to another
command, or sending an outcome to one of them. One continuation per outcome
*is* the outcome type — see [§12](core.md#12-error-continuations). A helper
that only computes with values — `at` above — stays an ordinary returning
`fn`.

A handler delimits `mu`. `k` holds the whole rest of the program, but a
jump to it replaces the running continuation only down to the nearest
handler the two have in common — the handler `k` was captured under, or the
copy of it a `resume` reinstated. So a clause that resumes twice gets both
answers back, even when the resumed code jumps to a `k` captured before it
performed:

```sl
effect Choose { fn flip() -> Bool; }

fn pick() -> String / {Choose} {
    let a = mu String { r <= <(match flip() { True => "H", False => "T" }) | r> };
    a
}

// "H T": `r` is the `let`'s own continuation, and each resumption has its own.
handle pick() {
    flip(): resume => <(<True | resume, " ") | add | x => (x, <False | resume) | add,
}
```

A jump made under a handler `k` was not captured under — one installed after
the capture, around code that was handed `k` — is an error at run time: "a
continuation left the handler it was captured under". The runtime's `IO`
handler sits under every program
([`IO`](effects.md#io-the-effect-the-runtime-handles)), so a `mu` anywhere in
`main` is delimited by it; `exit` is not a captured continuation, and leaves
from anywhere.

`reset e` delimits without handling. It is a handler with no clauses: it
answers no operation, so what `e` performs reaches the handlers around it,
and its value is `e`'s. What it adds is the boundary — a jump from inside it
to a continuation captured outside it is refused — so code run under `reset`
cannot leave through a continuation it was handed:

```sl
fn escape(k: -i64) -> i64 { <5 | k> }

mu i64 { out <= <(<out | escape) | out> }          // 5
mu i64 { out <= <(reset <out | escape) | out> }    // refused: the jump would leave the `reset`
```

A resumption whose slice crosses a `reset` carries a copy of it, as it does a
handler, so a continuation captured under the `reset` lands on the copy.

### Composable capture

`control::reset` is a library handler, not the bare `reset` delimiter. It
takes an explicit computation thunk. Inside it, `control::shift` receives a
callback whose argument is the captured, returning continuation:

```sl
command main | (exit: i32) / {IO} {
    let result = <fn {
        let value = <fn(resume: (i64 -> i64)) {
            <(<1 | resume, <2 | resume) | add
        } | control::shift;
        <(value, 10) | mul
    } | control::reset;
    <result | println;
    <0 | exit>
}
```

This prints `30`. Each resumption multiplies its argument by ten and returns
to the callback, which adds the answers. `mu` instead captures an abortive
consumer: cutting into it does not return to the cut site. Resumptions are
multi-shot and never memoize results.

The library uses `Shift<+A, +R, E>`. `A` is the operation's result and `R`
is the fixed answer type of one capture handler; both are positive.
The resumption has type `(A -> R / {..E})`. Its callback has type
`Delayed<((A -> R / {..E}) -> R / {..E}), ..E>`, so construction, callback
execution and resumption retain their separate demand points but share one
conservative effect budget `E`. Annotate an effectful resumption accordingly;
for example `(i64 -> i64 / {Factor})` when its continuation performs `Factor`.
Those effects escape to an outer handler rather than disappearing during
capture. Forcing an effectful callback happens when the capture handler
invokes it, not when it is passed to `shift`.

All captures at one installation share its `A` and `R`; this is not a
rank-polymorphic prompt. Nested capture handlers may use different types. Each handles its
own typed operations; incompatible answers at the same installation are
rejected. A thunk with no capture also works. Unhandled `control::shift` is
an effect error, including under bare `reset`. Always write the qualified
`control::reset` call: unqualified `reset e` retains its delimiter meaning.
`examples/effects/delimited.sl` runs all of it; `examples/errors/delimited_error.sl` is the
refused jump.
