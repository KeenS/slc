# Migration Guide

This guide covers the syntax changes made during and after the λ̄μμ̃
redesign. Removed forms are rejected by the current
compiler; sections show their replacements or explain compatible additions, and
every replacement is written in today's syntax, even where a later section is
what made it so.

## Generic effects and composable capture

Non-generic effects keep their spelling. Generic effects declare signed
parameters and write applications in rows:

```sl
effect Reader<+T> { fn read() -> T; }
fn get<+T>() -> T / {Reader<T>} { read() }
```

Handlers infer their application from the operations they intercept and the
answers their clauses supply. A stored handler can be annotated with
`Handler<i64, i64, {Reader<i64>}, {}>`. An incompatible same-name operation
is rejected even if a residual row or outer handler could accept it.
Invalid effect names and arguments are now rejected on unused signatures too.

For returning, multi-shot capture, pass a thunk to `control::reset` and use
`control::shift` inside it. Bare `reset e` is unchanged: it only delimits
abortive jumps and does not handle `Shift`. The callback's resumption
annotation must retain its effects, such as `(i64 -> i64 / {IO})`; resumptions
are no longer incorrectly treated as pure when their continuations perform
effects. See `examples/composable_capture.sl` and `examples/generic_effects.sl`.

## Inferred demand and nullary calls

Negative computations are call-by-name in every argument and component,
including when their polarity is inferred from later uses. Primitive direct
calls, flows and aliases now agree: positive arguments compute immediately,
negative arguments wait for demand. Outcome primitives demand only the
chosen callback. Use a separate `let+` to request eager construction instead
of relying on the old primitive-call exception.

A nullary returning function's name now has its actual runtime type: a
function accepting `(,)`. Naming it neither calls it nor charges its effects.

```sl
fn answer() -> i64 { 42 }
let factory = answer;
let first = answer();
let second = <(,) | factory;
```

Use `answer()` or `<(,) | answer` when a result is wanted. Parameterless
consumer transformers still name their direct consumers; naming is not
activation. Positive inputs now also run before computed final consumers,
consistently with ordinary stages and explicitly eager inputs. See
`examples/inferred_demand.sl` and the `flow_evaluation` regressions.

## Returning command exits

Explicit capture remains valid:

```sl
let result = mu i64 { out <= <path | __read_file | (
    select String { text => <(<text | str_len) | out> }
    & select String { reason => <0 | out> }
)> };
```

The equivalent yielding form supplies returning functions and omits `>`:

```sl
let result = <path | __read_file | (
    fn(text: String) { <text | str_len } & fn(reason: String) { 0 }
);
```

The chain can continue through `| println`. Every exit must return the same
type: do not mix a returning callback with a consumer. `select` remains
non-returning. `fs::real` and `fs::real_command` now use this adapter rather
than explicit `mu` captures. See `examples/yielding_commands.sl`.

## Stored handlers

Functions that install inline handlers remain valid. To store or select the
handler itself, use `handler Reader { clauses }` and `with value handle body`.
The type is `Handler<A, B, E, F>`: body, answer, handled effects, residual
effects. A `return` clause can change `A` to `B` even for a pure body.
`handler` and `with` are reserved syntax, and `Handler` is a built-in type
name; rename conflicting declarations. The old composition example's
`Handler` form is now `CommandSink`. See `examples/handler_values.sl`.

## Lists

The builtin list type `[A]`, the list patterns `[p, …]`, and the
`list_new`/`list_len`/`list_push`/`list_get` builtins are gone. `List<T>` is
an ordinary recursive enum in the stdlib's `list` module, with `length`,
`map`, `append`, and the outcome-offering `command nth` beside it.

Unsupported:

```sl
fn f(xs: [+i64]) -> i64 { … }
let xs = list_push(list_new(), 10);
```

Write:

```sl
use list::List::*;

fn f(xs: list::List<i64>) -> i64 { … }
let xs = Cons(10, Nil);
```

## Arm arrows

The arrow marks which side of the mirror the scrutinee is on: data flows
forward into an arm (`=>`), a demand reaches back into it (`<=`). `select`
matches data, so its arms now write `=>`; a `match` over a continuation
writes `<=`.

Unsupported:

```sl
select Colour { Red <= 0 | out, Green <= 1 | out }
match k { .retries(out) => .retries(out) }
```

Write:

```sl
select Colour { Red => <0 | out>, Green => <1 | out> }
match k { .retries(out) <= .retries(out) }
```

## Menu copatterns in `mu`

A `mu` menu arm now mirrors the field syntax of its `menu` declaration. The
item label precedes `:`, followed by the continuation binder. Repeating the
label as its binder may be shortened to the label alone. Every arm remains a
command and therefore writes `<=`.

Unsupported:

```sl
mu Config { .retries(out) <= 3 | out, .name => "slant" }
```

Write:

```sl
mu Config { retries: out <= <3 | out>, name: out <= <"slant" | out> }
mu Config { retries <= <3 | retries>, name <= <"slant" | name> }
```

Nested menu copatterns repeat the field shape: `tail: head: out <= c`.
An untyped, single bare arm remains the local continuation form
`mu { k <= c }`; use `item: item <= c` when constructing an untyped one-item
menu.

## Local `mu`

The parenthesised binder group is gone; `mu` is uniformly `mu [Type] { arms }`,
mirroring `select`. One binder arm captures the ambient continuation whole;
request arms build a menu. The decorative name is gone with the group, and
the type in front is what the expression *produces* (the old annotation's
dual).

Unsupported:

```sl
mu(k) { 42 | k }
mu here(k: -i64) { read_file(path, k, complain) }
```

Write:

```sl
mu { k <= <42 | k> }
mu String { k <= <path | fs::read | (k & complain)> }
```

## Function polarity

Every function now requires an arrow.

### Bare `fn`

Unsupported:

```sl
fn plus(x: +i32, y: +i32) { x + y }
```

Write:

```sl
fn plus(x: +i32, y: +i32) -> i32 { <(x, y) | add }
```

### `+fn`

The polarity prefix is removed.

Unsupported:

```sl
+fn plus(x: +i32, y: +i32) -> i32 { x + y }
```

Write:

```sl
fn plus(x: +i32, y: +i32) -> i32 { <(x, y) | add }
```

### `-fn`

A negative function consumes continuations and produces the continuation
written after the reverse arrow.

Unsupported:

```sl
-fn k(k1: -i32, k2: -i32) -> (i32, i32) { ... }
```

Write:

```sl
fn k(k1: i32 & k2: i32) <- (i32, i32) { ... }
```

The parameters before `<-` are the function's row, a menu of exits separated
by `&`. The function produces the consumer of the type written after `<-`.

## Consumer abstraction

A declaration that takes values and continuations is a `command`, and its two
kinds of parameter occupy separate parenthesized groups.

### Separate parameter groups

Unsupported:

```sl
command route(x: +i32, k: -i32) { k(x) }
```

Write:

```sl
command route(x: +i32) | (k: -i32) { <x | k> }
```

### `mu` is the expression, `command` is the declaration

For part of the redesign the declaration was spelled `mu`. It is `command`
again, and `mu` now names only the expression that captures the current
continuation — the two are different core constructs (`Λα. t`, a parameter the
caller supplies, against `μα. c`, the ambient continuation), and one keyword
for both hid that.

```sl
mu route(x: +i32) | (k: -i32) { x | k }          // old: the declaration
command route(x: +i32) | (k: -i32) { <x | k> }   // new

let source = mu { k <= <path | fs::read | (k & err)> };   // the capture
```

A declaration written with `mu` is a parse error naming the difference.

A `mu` expression binds one thing, the continuation it captures, and writes it
as its one arm:

```sl
mu | (k: -String) { … }   // old
mu String { k <= … }      // new
```

It has no value parameters: `mu(v) { … }` used to lower to `λv. …`, exactly
what `fn(v) { … }` lowers to. A binder whose body is a command rather than an
expression is `select`.

### Remove `to`

The directional `to` marker is removed.

Unsupported:

```sl
command route(x: +i32, to k: -i32) { k(x) }
```

Write:

```sl
command route(x: +i32) | (k: -i32) { <x | k> }
```

### An empty parameter group is left out

A `command` writes only the groups it has.

```sl
command main() | (exit: -i32) { … }   // old
command main | (exit: i32) { … }      // new

command log(message: +String) | () { … }   // old
command log(message: +String) { … }        // new
```

A local `mu` has no parameter group at all: `mu { k <= … }`.

### Bottom annotation

The declaration denotes a command, so its result is bottom. The annotation is
optional and does not change lowering:

```sl
command route(x: +i32) | (k: -i32) -> (;) { <x | k> }
```

## Partial agents

The old partial-agent forms are removed:

```sl
agent.to(k, h)
agent.consume(k, h)
fn.partial(a)
```

There is no accepted replacement. Construct the needed ordinary function or
negative function with explicit parameters instead.

## Removed type formers

### `Command<I, O>`

The `Command<I, O>` surface type former is removed. It was an internal
symmetric-agent convenience and is not part of the accepted λ̄μμ̃ surface.

Use the explicit polarity-bearing type forms instead:

```sl
(-I ; O)
```

For a function type, use:

```sl
(I -> O)
```

For a negative function declaration, use:

```sl
fn k(out: O) <- I { … }
```

### Expression-level `dual(e)`

Expression-level `dual(e)` is removed. It denoted the same witness as `e`
and relied on the checker to reinterpret its polarity, so it did not add a
distinct core term. The type-level `dual(A)` form remains available when an
explicit dual type is needed.

## Continuation activation

Activating a continuation is a cut, not a call. A call supplies an argument to
a function and returns; a cut sends a value to a consumer and does not return.

```sl
k(v)        // old
<v | k>     // new

EXIT(0)     // old
0 | EXIT    // later — and `EXIT` itself is now gone, see below
```

What flows in may be any value, a group included: `<(a, b) | add | k>`
sends the sum.
The consumer may be any expression that produces one, including a negative
function applied to its row: `<Color::Blue | code | answer>`.

A continuation passed to a function is an ordinary argument, since a
consumer is a value: `<k | handle`.

A cut has type `⊥`, so a `match` arm that ends in one leaves the type of the
match to the other arms, and code after a cut in a block is unreachable.

## The top-level `EXIT` is gone

Ending the program is a right a helper is handed, never one it takes: `exit`
reaches helpers as a continuation parameter or inside a consumer built where
it is in scope.

```sl
fn die(m: +String) -> ⊥ { println(m); 1 | EXIT }        // old

command main | (exit: i32) / {IO} {                      // new
    let die = select String { m => { <m | println; <1 | exit> } };
    …
}
```

## The entry point is a `command`

A program is a command, so `main` is a `command` that takes no values and one
continuation — its exit status:

```sl
fn main() -> i32 {          // old
    println("hi");
    42
}

command main | (exit: i32) / {IO} {  // new
    <"hi" | println;
    <0 | exit>
}
```

The final value is no longer printed: a program's output is exactly what it
prints, and its status is what it sends to `exit`. A `main` that used to end
with a value must print it. Inside `main`, cut against the `exit` parameter
rather than the global `EXIT` — a `command` body must reach a continuation on
every path, so a `main` that falls off the end is refused by the type checker.

## `select` covers any positive type

`select` is no longer restricted to enums, its arms use the same patterns
`match` uses, and an arm is written `pattern => command`:

```sl
select Color {                     // one arm per variant — the negative additive
    Red => <0 | return>,
    Green => <1 | return>,
}

select Reading {                   // one arm, binding every field — the negative multiplicative
    Reading { value, unit } => <(<value | int_to_str, unit) | add | out>,
}

select (+i64, +i64) {              // a bare product names its type
    (left, right) => <(left, right) | add | out>,
}
```

Two things changed. An arm's shape used to be a variant name with an optional
payload binder; it is now a pattern, so a struct or tuple shape is written the
way `match` writes it. And an arm used to be written the other way round, as
`command => pattern`:

```sl
select Color {
    0 | return => Red,     // old
    Red => <0 | return>,   // new
}
```

The shape now comes first, where a `match` puts it, and `=>` points from it to
the command — data flows forward into the arm. A `match` arm produces a value
from a shape, and a `select` arm runs a command when a shape arrives. Writing
the old order is a parse error that says so.

## Struct literals and patterns

A struct literal is an ordinary expression — it used to parse only as a call
argument — and a struct pattern now binds its fields at run time, which it
silently failed to do. A struct value is a labelled product, the same shape an
`enum` variant has.

## Fallible builtins

A builtin that can fail no longer aborts the program: it takes the
continuation each outcome belongs to.

```sl
let content = read_file(path);          // old: a runtime error if it fails

<path | fs::read | (                           // new
    select String { content => ... }
    & select String { message => ... }
)>
```

The same applies to `write_file`, `char_at`, `list_get`, `map_get`, and
`find_char`. `__parse_int` is renamed `parse_int`, and its failure
continuations now receive a message rather than the original input.
`str_to_int` is removed; use `parse_int`.

An out-of-range `s[i]` and a division by zero stay fatal: they are operator
syntax with nowhere to put a continuation, and they report a bug rather than a
case to handle.

## Values and continuations together

A declaration that takes both values and continuations is a `command`, not a `fn`:

```sl
// old: a positive function carrying a consumer and returning a position
fn parse_value(input: +String, pos: +i64, report: -ParseResult) -> i64 { ... }

// new: a command with a value group and a continuation group
command parse_value(input: String, pos: i64) | (ok: i64 & report: ParseResult) { ... }
```

The body of a `command` ends in a cut rather than returning: what was a returned
position is sent to `ok`. Where the old code sequenced two fallible steps with
`let`, the new code passes the rest of the work as a continuation:

```sl
// old
let value_end = parse_value(input, pos, report);
...

// new
<(input, pos) | parse_value | (select i64 { value_end => ... } & report)>
```

A helper that only computes with values stays an ordinary positive `fn`.

## `|` is flow, `<>` is the cut, and `@` is gone

Everything moves left to right through one operator, and every step
composes. Brackets say where a chain is closed, and the stage beside a
bracket takes its role from that — so nothing has to be inferred:

```sl
v @ k                          // old
<v | k>                        // new — the cut

then(f, k)                     // old — the prelude combinator, now deleted
f | k                          // new — the same consumer, composed

21 @ then(double, out)         // old
<21 | double | out>            // new
```

`<` marks the value flowing in, and `>` closes a chain: the stage before it
consumes. A chain without `<` begins with a function and composes, so
`f | k>` composes into `k`, while `<f | k>` sends `f` to it (see "`<` is
never left out"). A chain is flat, because composition is associative. `@` keeps its other
job, the as-pattern binder `c @ '0'..='9'`.

## A function is applied by flowing into it

`f(a)` is gone: an application is `<a | f`, and several arguments are the
product they always were.

```sl
println(label(area(shape)))            // old
<shape | area | label | println        // new

add(a, b)                              // old
<(a, b) | add                          // new
```

Constructors still build — `Cons(h, t)` is unchanged. Everything else
flows, a `command` included: its values come from the chain and its exits
are the closing stage.

```sl
nth(xs, 2, found, missing)             // old
<(xs, 2) | nth | (found & missing)>    // new
```

## Calls and rows are unary

A declaration binds one argument per group, so the parameters of a group
are its parts: `f(a, b)` is `f((a, b))`, and a continuation row is one
menu of exits. Nothing written changes, but a row is now a value — it can
be passed whole:

```sl
command nth(xs: List<T>, i: i64) | (found: T & missing: String)

<(xs, 2) | nth | (found & missing)>          // the exits, as one menu
command forward(…) | (row: (T & String)) {   // or handed on unopened
    <(xs, 2) | nth | row>
}
```

## A stdlib helper with values and continuations is a `command`

`traced` was a positive `fn` returning `-T`, so a caller built the consumer
and then cut into it. It takes values *and* continuations, which is what a
`command` is, so it is one — and the call reads like every other call:

```sl
42 | (("answer", out) | traced)>          // old
<("answer", 42) | trace::tap | out>       // new
```

`defaulting(fallback, k)` is gone rather than converted. Its job was to
*be* a consumer in a row slot, and a command is not one; the slot takes
`select String { m => <fallback | k> }`, which is what the combinator was
hiding.

```sl
(xs, 9) | nth | (out & (0, out) | defaulting)>              // old
<(xs, 9) | nth | (out & select String { m => <0 | out> })>  // new
```

## Printing is an effect

`println` and `print` perform the `IO` effect the prelude declares, so a
declaration that prints carries `{IO}` in its row — `main` included. The
runtime installs the handler, so `main` may leave it undischarged and
nothing else may:

```sl
command main | (exit: i32) {                    // old
    "hi" | println;
    0 | exit>
}

command main | (exit: i32) / {IO} {             // new
    <"hi" | println;
    <0 | exit>
}

fn greet(name: String) -> (,) / {IO} { <("hello, ", name) | add | println }
```

A program can now handle its own output: a `handle` with a `write_line`
clause sits nearer the operation than the runtime's handler and answers
first. See `examples/io.sl`.

This bites where it did not before because a pipeline stage now charges
its effects at all: `<x | throw` was silently free, and only the old call
form `throw(x)` was counted.

## `<` is never left out

A chain used to read its head as a value unless the head was a function, and
`<` was needed only to send a function on as a value. Now `<` always marks
what flows in, and a chain without it begins with a function, whatever its
head is:

```sl
"hi" | println;             // old: an application
<"hi" | println;            // new

0 | exit>                   // old: a cut
<0 | exit>                  // new

(a, b) | f                  // old: an application of `f`
<(a, b) | f                 // new

f | k>                      // composition into a consumer, as before
<f | k>                     // `f` itself sent to `k`, as before
```

A head that is not a function, written without `<`, is refused, pointing at
the missing bracket.

## Nesting is significant

A tuple, a sum, a menu of exits and a form value hold as many components as
are written, and a nested one is a component. A nested tuple and a flat one
used to be the same value:

```sl
let t = (1, (2, 3));
t.1                        // old: 2 — projection saw the flat spine
t.1                        // new: (2, 3)
t.2                        // old: 3; new: refused, `t` has two components

fn f(p: (i64, i64, i64))   // takes (1, 2, 3), and no longer (1, (2, 3))
```

An alternative is built by its position alone, so `::1(v)` no longer needs
its sum known by the end of its declaration; `(A | (B | C))` has two
alternatives, and its values are `::0(a)`, `::1(::0(b))` and `::1(::1(c))`.

## Types are joined by ASCII connectives

The glyphs are gone from the surface: `⊗` is `,`, `⅋` is `;`, and `⊥` is
`(;)`. A parenthesised type joins any number of components with one
connective, and `⊗` no longer multiplies.

```sl
fn sum_pair(p: (i64 ⊗ i64)) -> i64                 // old
fn sum_pair(p: (i64, i64)) -> i64                  // new

command consume | (k: (-i64 ⅋ -i64)) { … }         // old
command consume | (k: (-i64 ; -i64)) { … }         // new

fn stop(k: -i32) -> ⊥ { 0 | k⟩ }                   // old
fn stop(k: -i32) -> (;) { <0 | k> }               // new

2 ⊗ 3                                              // old
<(2, 3) | mul                                      // new
```

## The logical units are the nullary connectives

The prelude's `Unit`, `Bottom`, `Empty` and `Top` are gone. Each unit is its
connective's nullary spelling, a paren holding only the separator:

```sl
fn f() -> Unit { (,) }                                // old
fn f() -> (,) { (,) }                                 // new

command main | (exit: i32) -> Bottom / {IO} { … }     // old
command main | (exit: i32) -> (;) / {IO} { … }        // new

fn absurd(out: i64) <- Empty { select Empty {} }      // old
fn absurd(out: i64) <- (|) { select (|) {} }          // new

fn top() -> Top { mu Top {} }                         // old
fn top() -> (&) { (&) }                               // new
```

`Unit {}` and the `Bottom {}` demand are `(,)`. A declaration named `Unit` or
`Bottom` is now an ordinary one, with nothing recognised by its name.

## A sum can be written without declaring an enum

`(T1 | T2)` is an enum written anonymously, and its values name their
alternative by position, counted from 0:

```sl
enum Outcome { Number(i64), Text(String) }                // old: a declaration
fn show(x: Outcome) -> String {
    match x { Outcome::Number(n) => <n | int_to_str, Outcome::Text(s) => s }
}

fn show(x: (i64 | String)) -> String {                    // new
    match x { ::0(n) => <n | int_to_str, ::1(s) => s }
}
```

A position is read against the sum its context gives, so `::1(v)` needs a
return type, an annotation, a parameter or a cut to say which sum it is in.

## A form value is written from its continuations

`(k1 ; k2)` is new: a value of `(T1 ; T2)` that, fed a product `(a, b)`, hands
`a` to `k1` and then `b` to `k2`. Nothing that compiled before changes.

## A joint value fits the other spelling of its type

A slot declared at one spelling of `(A ; B)` used to refuse a value written at
the other, even though they are one type, so a declaration had to match its
implementation's spelling:

```sl
menu Deliver { deliver: (-String -> -i64) }     // old: forced to match the negative fn
fn deliver_i64(out: String) <- i64 { … }

menu Deliver { deliver: (i64 -> String) }       // new: either spelling
```

Nothing that type-checked changes meaning — the written spelling is tried
first. Inside a tuple or a type argument the spelling still has to match.

## `use m::*` brings a module's `pub` members

A glob used to mean an enum's variants only. Over a module it now brings
every `pub` member in bare:

```sl
use num::min;                      // old: one name at a time
use num::max;

use num::*;                        // new: every `pub` member of `num`
```

A named `use` and the module's own declarations win over a glob. Two globs
bringing the same name are allowed until that name is used; then write the
path, or `use` the one you mean by name.

Only the stdlib modules a program reaches are loaded now, so a program that
names no module is checked against the prelude alone. Nothing to change —
but a program relying on a stdlib name *without* its path or `use` was
already an error, and stays one.

## A call is not applied to part of its group

A stage supplies a callee's whole value group. Giving a declared function
or command fewer values used to type-check — its `;`-nested type presented
the first parameter alone — and then crashed at run time with "a consumer
of 2 components received …". It is refused at check time now:

```sl
let inc = 1 | add;                    // was accepted, then crashed
<(1, 2) | add                         // the call

<"high" | route                       // refused: `route` takes (String, i64)
<("high", 7) | route | k>             // the call, closing on its exits
```

A program's own function named like a builtin — `fn add` — is now checked
as the declaration it is. It used to inherit the builtin's exemptions by
name.

## `result::Result` is `either::Either`

The two-way sum is named for what it is rather than for one use of it:

```sl
result::Result::Ok(1)            // old
result::Result::Err("no")

either::Either::Left(1)          // new
either::Either::Right("no")
```

`Result<T, E>` becomes `Either<L, R>`, and `Ok`/`Err` become `Left`/`Right`
in the same order. Neither side means success; a program that wants that
reading says so in its own names.

## Library names are carried by their module

A stdlib name no longer repeats the module it lives in. Call through the
path — the module is the context — rather than importing a name that only
made sense with its suffix:

| old | new |
|---|---|
| `seq_of_list`, `list_of_seq`, `seq_of_stream` | `seq::of_list`, `seq::to_list`, `seq::of_stream` |
| `map_seq`, `filter_seq`, `take_seq` | `seq::map`, `seq::filter`, `seq::take` |
| `SeqStep` | `seq::Step` |
| `map_stream`, `zip_stream`, `drop_stream` | `stream::map`, `stream::zip`, `stream::drop` |
| `read_file`, `write_file`, `open_file`, `close_file`, `file_exists` | `fs::read`, `fs::write`, `fs::open`, `fs::close`, `fs::exists` |
| `traced` | `trace::tap` |

`take_while`, `read_line`, `count_from`, `length`, `unwrap_or` and the rest
were already names that do not restate their module, and are unchanged.

```sl
use seq::map_seq;                                        // old
(double, s) | map_seq

<(double, s) | seq::map                                  // new
```

## File operations are the `fs` module's

The six file builtins are no longer names a program has unasked. They are
the stdlib's `fs` module — commands over runtime primitives renamed
`__read_file` and so on, which a program is not meant to call:

```sl
path | read_file | (ok & failed)>              // old

<path | fs::read | (ok & failed)>              // new
```

Their outcome rows are unchanged; their names lost the `_file` the module
now carries — see the next entry.

## The prelude shrank; the rest is a stdlib you `use`

Only `IO` and `Display` (with `fmt`/`to_string`) stay in scope unasked.
`List`, `Option`, `Result`, `min`/`max`/`abs`, `Stream`, `Seq`, `Lazy` and
`traced` (now `trace::tap`) moved to stdlib modules, reached by path or brought in with `use`:

```sl
let xs = Cons(1, Cons(2, Nil));                      // old: in scope unasked
xs | length | println;

use list::List::*;                                   // new
use list::length;
let xs = Cons(1, Cons(2, Nil));
<xs | length | println;
<(3, 7) | num::min | println;                        // or by path, no import
<("answer", 42) | trace::tap | out>
```

A name that is neither local, declared, nor imported is now a checker
error at the use — "`min` is not defined here" — rather than a runtime
"unbound variable"; likewise a signature naming an unknown type, at the
declaration. A program that needs `exit` inside a helper passes it in, as
`main` does: it was never a global, and the checker now says so.

## A module's declarations are private unless `pub`

A `mod` used to expose everything it declared. It now exposes what is
marked, as in Rust:

```sl
mod geometry {
    enum Shape { … }                   // old: reachable everywhere
    fn area(s: Shape) -> i64 { … }

    pub enum Shape { … }               // new
    fn squared(n: i64) -> i64 { … }    // new: private, the module's own
    pub fn area(s: Shape) -> i64 { … }
}
```

Private means reachable by the declaring module and the modules nested
inside it. A declaration in no module is visible everywhere, so a
single-file program needs no `pub` anywhere.

## A binder is a pattern

`let p = e` and a parameter `p: T` take a pattern, as in Rust; a bare name
is the trivial one, so nothing written before needs changing. What is new
is that a binder may take its value apart:

```sl
let pair = make(); let a = pair.0; let b = pair.1;   // old
let (a, b) = make();                                 // new

fn norm(p: Point) -> i64 { <(p.x, p.x) | mul | sum => (sum, <(p.y, p.y) | mul) | add }   // still fine
fn norm(Point { x, y }: Point) -> i64 { <(x, x) | mul | sum => (sum, <(y, y) | mul) | add }
```

A binder must be irrefutable — it stands for every value of its type — so a
many-variant enum is still taken apart with `match`. A continuation
parameter stays a name: control leaves through it.

## Handler clauses bind their continuation after a colon

An operation is a demand, and its clause binds the carried continuation
the way a copattern does — after a colon, under any name — or omits it
when the clause never resumes. The bare word between pattern and arrow is
gone:

```sl
throw(m) resume => 0 - 1               // old
throw(m) => -1                         // new: never resumes, no binder

config() resume => resume(10)          // old
config(): resume => <10 | resume        // new: bound after the colon
```

## Effect rows: row variables, written like generics

A higher-order function forwards an argument's effects by declaring a
**row variable** — a generic parameter used with the `..` "rest" spelling
in a row, on its own arrow and on the parameter's:

```sl
fn map<+A, +B>(f: (A -> B), xs: List<A>) -> List<B>              // old: f had to be pure
fn map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}   // new
```

A bare arrow still means pure — now enforced against arguments too:
passing an effectful function where a rowless arrow is declared is an
error at the call site. `{Exn, ..E}` extends a variable; `main`'s row is
`{IO}` or empty.

## Shifts removed: a consumer is a value

The polarity shifts `↓`/`↑` are gone. They erased at lowering, and the
declared negatives (`menu`, `form`) already crossed the data seam bare, so
the box taxed only structural negatives. A consumer now travels bare in
every data position — an enum payload, a record field, a `fn` value
parameter — and the glyphs simply come off:

```sl
enum Choice { Refutes(↓-i64) }     // old
enum Choice { Refutes(-i64) }      // new

Choice::Refutes(↓k)                // old
Choice::Refutes(k)                 // new

Refutes(r) => 42 | ↑r              // old
Refutes(r) => <42 | r>            // new

fn describe(note: ↓-String) -> ⊥ { "…" | ↑note }   // old
fn describe(note: -String) -> (;) { <"…" | note> }  // new
```

With no box to go through, `dual` is an involution on the nose: `-(-T)`
*is* `T`, and double-negation elimination is `fn dne<+T>(t: -(-T)) -> T
{ t }`. The one rule that remains is orientation: the left of `|` is the
value side, so a continuation is passed as an argument, never cut against
data. Where a named box is still wanted, declare it — `menu Lazy<+T>
{ force: T }` is the computation returning `T`, and a one-field `form` is
a named, storable consumer.

## Additive control

### `select`

The old experimental `select` declaration no longer defines a new type.
The final form consumes an existing enum:

```sl
enum Color { Red, Green, Blue }

fn k(return: i32) <- Color {
    select Color {
        Red => <0 | return>,
        Green => <1 | return>,
        Blue => <2 | return>,
    }
}
```

Arms must cover each enum variant exactly once. An arm binds the payload of
its variant when the variant declares one, and passes it to that arm's
consumer:

```sl
enum ParseResult { Parsed(String), Failed(String) }

fn deliver(ok: String & err: String) <- ParseResult {
    select ParseResult {
        Parsed(text) => <text | ok>,
        Failed(message) => <message | err>,
    }
}
```

A pair of success and failure continuations threaded through every function
can therefore be replaced by one continuation that accepts a result enum.

### `choose`

Old variant-style `choose T { Variant }` syntax is removed, and so is the
experimental struct-based `choose Struct`. No `choose` form exists.

## A cut sends its value

`t | k` used to lower to a μ binder that shadowed `k`, so the value was sent
nowhere. The cut `<t | k>` now delivers `t` to `k`, as it always claimed to.

## A `match` arm has no guard

`p if e => …` is gone. A test on what a pattern bound is a `match` inside the
arm:

```sl
match n { m if m > 0 => <m | ok>, _ => <0 | ok> }   // old
match (<(n, 0) | gt) { True => <n | ok>, False => <0 | ok> }   // new
```

The exhaustiveness diagnostic says "add a `_` arm" rather than "an unguarded
`_` arm".

## A bundle item that ends in a cut is refused

Every item of a bundle is built with it, and the consumer chooses one after,
so an item that ends in a cut jumped before anything chose it. It is refused;
write the consumer it meant:

```sl
<c | choose | ({ <0 | exit> } & { <1 | exit> })>               // old: took the first exit
<c | choose | (fn(_) { <0 | exit> } & fn(_) { <1 | exit> })>   // new
```

## A builtin stage is held to its signature

What flows into a builtin is checked as it is for a declared function —
against the builtin's signature, with its whole value group:

```sl
<(1, "b") | add        // old: accepted, then failed at run time; now refused
<1 | add               // old: a partial application; now refused
<("a", "b") | add      // old: joined the strings; now refused
<("a", "b") | str_concat
```

## There is no `if`

A choice on a `Bool` is a `match` on its two variants:

```sl
if n > 0 { n } else { 0 - n }                          // old
match (<(n, 0) | gt) { True => n, False => <n | neg }   // new

if a { x } else if b { y } else { z }                  // old
match a { True => x, False => match b { True => y, False => z } }   // new
```

An `if` with no `else` yielded unit on the false path; write that arm
explicitly, `_ => (,)`.

## `!` is the prelude's `not`

Logical not is an ordinary function a `bool` flows into.

```sl
!done                  // old
<done | not            // new
```

## `&&` and `||` are gone

A conjunction or a disjunction is a `match` on its left side, which runs the
right side only when it is needed:

```sl
ok && <x | valid                                   // old
match ok { True => <x | valid, False => False }    // new

a || b                                             // old
match a { True => True, False => b }               // new
```

## `println` and `print` render through `Display`

The two were builtins that took any value and quoted a string. They are now
prelude functions over `<T: Display>`: a call is a flow like any other, a
string prints as itself, and a value prints only if its type has `Display`.

```sl
println(x);            // old: accepted as a builtin
<x | println;          // new

<"hi" | println;       // prints `hi`; the builtin printed `"hi"`
```

The prelude has `Display` for the base types, for the unit, and for tuples
and choices up to eight components.

## A type parameter states its polarity

Every generic parameter of a `fn`, `command`, `impl`, `enum`, `data`, `menu`
or `form` is declared `+` or `-`. A row variable, used as `..E`, keeps no
mark.

```sl
enum List<T> { Nil, Cons(T, List<T>) }                  // old
enum List<+T> { Nil, Cons(T, List<T>) }                 // new

fn map<A, B, E>(g: (+A -> +B / {..E}), x: A) -> B / {..E}    // old
fn map<+A, +B, E>(g: (+A -> +B / {..E}), x: A) -> B / {..E}  // new
```

A missing mark is an error naming the parameter. Mark `+` each parameter
that is not a row variable, then `-` each one the checker reports being
given a function, a consumer, a menu or a form:

```sl
fn label<+T: Describe>(x: T) -> String { <x | describe }   // old, given a menu
fn label<-T: Describe>(x: T) -> String { <x | describe }   // new
```

A generic that took both data and codata splits in two.

## A `let` says when it computes

`let+` computes its value where it is written, and `let-` binds the
computation to run wherever its result is demanded. A plain `let` follows
the polarity of its type: a computation of negative type — a block or call
producing a function, a consumer or a menu — is no longer run where it is
written but at each use, effects included.

```sl
let shout = { <"made" | println; fn(s: String) { <s | println } };
<"a" | shout;       // old: made, a       new: made, a
<"b" | shout;       // old: b             new: made, b

let+ shout = { <"made" | println; fn(s: String) { <s | println } };   // runs once
```

A `let` of a computation whose type inference cannot tell its polarity is
an error; annotate it, or write `let+` or `let-`.

## A lambda's parameter has a known polarity

A lambda parameter left unannotated must have its type — at least its
polarity — fixed by how the lambda is used by the end of the declaration.
A lambda nothing pins down no longer generalizes; annotate it, or declare
the polymorphic function.

```sl
let f = fn(x) { x };               // old: generalized; now refused
fn id<+T>(x: T) -> T { x }         // new: a declaration

let inc = fn(x) { <(x, 1) | add };  // still accepted: the literal fixes `x`
```

## Arithmetic and comparison are trait methods

`add`, `sub`, `mul`, `div`, `rem`, `neg`, `eq`, `ne`, `lt`, `gt`, `le` and
`ge` are now methods of the prelude's `Add`, `Sub`, `Mul`, `Div`, `Rem`,
`Neg`, `Eq` and `Ord`, with impls for each integer width, and `Add`, `Eq`
and `Ord` for the other base types they apply to. A value flows into them as
before, and a pair of strings is joined again:

```sl
<(1, 2) | add          // dispatches to `i64`'s `add`
<("a", "b") | add      // `String`'s `add`: "ab"
```

The builtins beneath them are `__add` and so on; a program that named a
builtin directly names the method instead.

## Infix operators, prefix `-` and indexing are gone

Arithmetic and comparison are the prelude's trait methods, and a group flows
into them. A run of operators is one chain of binder stages; a compound
operand is its own parenthesised chain.

```sl
a + b                    // old
<(a, b) | add            // new

a * b + c                // old
<(a, b) | mul | x => (x, c) | add       // new

match n > 0 { … }        // old
match (<(n, 0) | gt) { … }             // new

-x                       // old
<x | neg                 // new; `-1` is still a literal

s[i]                     // old
<(s, i) | index          // new
s[i..j]                  // old
<(s, i, j) | substring   // new; an open end is `0` or `(<s | str_len)`
```

An integer literal takes its width from the other operand, as before:
`<(x, 1) | add` for `x: i32` is `i32`'s `add`.

## `bool`, `true` and `false` are the prelude's `Bool`

The built-in boolean is gone. The prelude declares `enum Bool { False, True }`,
and a program names the type and its variants; the old spellings are refused
with the names that replace them. A `match` on both variants is exhaustive,
so the `_` arm a `bool` match needed can name `False`.

```sl
fn positive(n: i64) -> bool { match (<(n, 0) | gt) { true => true, _ => false } }   // old
fn positive(n: i64) -> Bool { match (<(n, 0) | gt) { True => True, False => False } } // new
```

A `Bool` still prints as `true` or `false`.

## A chain is `<value | stage | consumer>`

`⟨` and `⟩` are gone. With no operator left to read them, `<` marks what
flows into a chain and `>` the consumer that closes it, and the old brackets
are refused with the new ones.

```sl
⟨x | f | k⟩              // old
<x | f | k>              // new

⟨-1 | k⟩                 // old
<-1 | k>                 // new: a number touching `<-` opens the chain
```

The core's own cut, `⟨ v ∥ k ⟩`, is unchanged.

## A `mu` continuation is delimited by its handler

A jump to a `mu` continuation used to replace the whole running stack. It
now replaces it down to the nearest handler the jump and the capture have in
common, so a clause that resumes more than once gets every answer back when
the resumed code jumps to a continuation it captured earlier:

```sl
effect Choose { fn flip() -> Bool; }

fn pick() -> String / {Choose} {
    let a = mu String { r <= <(match flip() { True => "H", False => "T" }) | r> };
    a
}

handle pick() {
    flip(): resume => <(<True | resume, " ") | add | x => (x, <False | resume) | add,
}
// old: "H" — the first jump to `r` left the clause
// new: "H T"
```

A jump from under a handler installed after the capture, by code that was
handed the continuation, used to leave that handler silently; it is now a
run-time error, "a continuation left the handler it was captured under".
Return the continuation out of the `handle` and jump to it there, or pass
the value out instead of jumping:

```sl
let r = handle (<k | use_inside) { … };    // old: a jump to `k` inside left the handler
let r = handle use_inside() { … };         // new: `use_inside` returns; the jump is outside
<r | k>
```

## `reset` is a reserved word

`reset e` delimits a computation without handling it (`DESIGN.md` §6), so
`reset` no longer names a variable, a function, a field or an operation:

```sl
let reset = 0;           // old
let reset_count = 0;     // new
```

## A delayed computation performs where it is used

A computation in a by-name position used to be charged where it was written,
so a handler around the writing seemed to answer it even when it ran after
that handler had returned, and the run failed with "no handler for
operation". Its effects are now charged at each use, under the handlers
there. A program that relied on the handler around the writing runs the
computation there with `let+`:

```sl
let g = handle { let- f = make(); f } { throw(m) => fn(n: i64) { 0 } };   // old: failed at run time
let g = handle { let+ f = make(); f } { throw(m) => fn(n: i64) { 0 } };   // new
```

## Effect rows are part of types

A row now rides on the type of the value that performs it, instead of being
followed by name. What a program performs is unchanged; where it is charged
follows the value:

- A `fn` performs its body's effects where it is called, not where it is
  written. A lambda written inside a `handle` and called after it is refused
  unless a handler answers the call; one written outside and called under a
  handler is accepted.

  ```sl
  let f = handle { fn(x: i64) { <"late" | throw } } { … };   // old: accepted, then failed at run time
  <1 | f | println;                                          // new: refused, `main` performs `Exn`
  ```

- A function bound again by `let`, or a higher-order global handed on as a
  value, keeps its row: calling it performs what the original does.
- A delayed computation carries its row on its type. Stored in a tuple or
  handed on, it is accepted, and performs where it runs; passed where a pure
  arrow is declared, it is refused. `let+` still runs it where it is written.
- An exit accepts any row: a consumer handed to a command's continuation
  parameter performs where it is handed over, as before.
- Two menus that declare latent rows may now share an item name: a demand's
  type says which menu it is on.

## File operations are an effect

Touching a file performs the `fs` module's `Fs` effect rather than reaching
the disk behind `IO`, so a declaration that reads or writes files says
`{fs::Fs}` in its row, and the code that touches files runs under a handler:
`fs::real` for the disk, or one of the program's own. `main` still leaves
only `IO`; a program installs `fs::real` itself.

```sl
command main | (exit: i32) / {IO} {                               // old
    <"input.txt" | fs::read | (select String { t => { <t | print; <0 | exit> } } & complain)>
}

command main | (exit: i32) / {IO} {                               // new
    <(,) | fs::real_command | (fn {
        <"input.txt" | fs::read | (select String { t => { <t | print; <0 | exit> } } & complain)>
    })>
}
```

The commands keep their shapes. A handler of the program's own answers the
operations, `fs::read_file` and its siblings, each with a sum of its outcomes:

```sl
fn canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {..E} {
    handle <(,) | program { fs::read_file(path): resume => <::0("canned") | resume }
}
```

## More values are turned to the other spelling of `;`

Nothing that compiled changes meaning. A tuple or an alternative written out
may now hold a component at the other spelling of its declared type, and a
stage's result may meet the next stage at the other spelling, where both
used to be refused:

```sl
let t: ((i64 -> String), i64) = (deliver_i64, 1);   // new: accepted
<(,) | get_negative | use_it | println;             // new: accepted
```

A bounded negative function, `fn emit<+T: Display>(out: String) <- T`, now
stands as a stage read either way round, and a consumer flowing into a
negative trait method is its continuation, `<s | deliver`. A type
constructor's arguments still have to match: `List<(A ; B)>` is not
`List<(B ; A)>`.

## A type variable takes the polarity of the parameters it meets

A type that nothing solves still has a polarity once it meets a generic
parameter with a sign. A lambda parameter used only where `<+T>` is declared
is positive, and no longer asks for an annotation; one type meeting both a
`<+T>` and a `<-T>` is refused, where it used to be accepted and fail when
run:

```sl
fn keep<+T>(x: T) -> (,) { (,) }
fn feed<-T>(x: T) -> (,) { (,) }

let f = fn(x) { <x | keep };            // old: "polarity is not known"; new: accepted
let g = fn(x) { <x | keep; <x | feed }; // new: "no type is both positive and negative"
```

## A computation takes no dummy parameter, and a program ending in a cut is handled

Nothing that compiled changes meaning. `fn { … }` is `fn(_: (,)) { … }`, and
`fs::real_command` runs a program that leaves through continuations of its
own, so file work no longer returns a status through `mu i32 { done <= … }`:

```sl
let status = <(fn(u: (,)) {                                       // old
    mu i32 { done <= <"input.txt" | fs::read | (select String { t => { <t | print; <0 | done> } } & complain)> }
}) | fs::real;
<status | exit>

<(,) | fs::real_command | (fn {                                   // new
    <"input.txt" | fs::read | (select String { t => { <t | print; <0 | exit> } } & complain)>
})>
```

A command's exit parameter that writes a row, `program: ((;) / {..E})`, now
takes what the exit handed to it performs, where it used to be charged at
the call; and a `handle` whose body is `(;)` runs it.

## A menu or form takes a row parameter

A row variable on a menu or form declaration was refused. It is now one of
the declaration's row parameters, declared without a sign, and each use
gives its row: `Seq<T, ..E>`, or `Seq<T>` for none. The stdlib's `Seq` takes
one, so a sequence built with an effectful function performs where its steps
are demanded, and `seq::map`, `filter` and `take_while` no longer build their
rest with `let+`:

```sl
fn map<+A, +B, E>(f: (A -> B / {..E}), s: Seq<A>) -> Seq<B> / {..E}      // old
fn map<+A, +B, E>(f: (A -> B / {..E}), s: Seq<A, ..E>) -> Seq<B, ..E>    // new
```

A function that demands a sequence's steps forwards its row:
`seq::to_list(s: Seq<T, ..E>) -> List<T> / {..E}`.

## Returned effects stay on the returned type

A constructor's row describes constructing its result. A returned consumer,
menu or form carries its own effects, performed when applied or demanded:

```sl
fn make(out: -i64) -> -i64 / {Tick}             // old, refused
fn make(out: -i64) -> (-i64 / {Tick})           // new
```

Declare a menu or form's latent row, or give its row parameter at each use.
`Stream<T, ..E>` now carries its demand effects through `tail`, mapping and
the sequence bridges. `stream::take` performs that row while producing a
list; `seq::of_stream` and `seq::take_while` retain it on their lazy result.
A handler belongs around the demand that runs the effect, not merely around
the constructor. Delay remains call-by-name and does not cache a result.

### Record fields and positional choices are by-name positions

Negative computations in record fields and `::i(e)` payloads now follow the
same rule as tuple components and named variant payloads: they run on demand,
not during storage. Positive components still compute during construction.
Projection, matching and aliasing do not force a stored negative computation.

```sl
data Holder { callback: (i64 -> i64 / {Build}) }
let saved = Holder { callback: make() };
let choice: ((i64 -> i64 / {Build}) | i64) = ::0(make());
```

If `make()` performs `Build` before returning a function, these components
must retain `Build`, even if a handler surrounds the record or choice
construction. Handle each later demand instead. To retain eager construction
of the callback, write `let+ callback = make();` separately and store
`callback`; an eager binding of the containing record is not a recursive force.
See `examples/by_name_components.sl` for a complete runnable example.

### Projection demands a delayed bundle

Projecting `pending.0` from a delayed bundle now constructs the bundle rather
than failing with "projection of component 0 from <delayed>". Each projection
repeats construction under its current handlers. The selected item is passed
on as stored: projection does not additionally force a delayed callback.

```sl
let pending = (build() & 0);
let first = handle pending.0 { build(): resume => <10 | resume };
let second = handle pending.0 { build(): resume => <20 | resume };
```

Here `build()` returns an integer; `first` is `10`, and `second` is `20`.
The bundle's construction row is required at projection, while a negative
item retains its own row for later demand. See `examples/delayed_bundle.sl`
for both effect boundaries in a complete program.

## Removed constructs

Runnable examples of the updated handler, exit, consumer and stream rules
are collected in [the examples guide](../examples/README.md).

### Partial handlers require explicit forwarding

A handler must answer every operation of each effect it names. To intercept
only some operations, add a final `_ => forward` clause. The effect remains
in the outward row, so supply an outer handler. A standalone mock must
provide all operation clauses; a partial `fs::read_file` mock no longer
silently discharges all of `fs::Fs`.

### Command exits preserve their effects

A pure exit slot no longer accepts an effectful consumer. To forward to an
arbitrary consumer, write its latent row and charge it when activated:

```sl
command send<E>(value: i64) | (out: (-i64 / {..E})) / {..E} {
    <value | out>
}
```

Store or return that consumer with the same latent row. A handler around
handover cannot discharge an activation that happens later. An unused exit
may have a row parameter without adding it to the command's own row.
`fs` commands, `list::nth`, `trace::tap` and builtin outcome commands now
preserve their exits' rows explicitly.

### Separate construction and activation

A stored effectfully constructed function now names both phases:
`Delayed<(i64 -> i64 / {Use}), {Build}>`. The old combined annotation
`(i64 -> i64 / {Build, Use})` cannot represent effectful construction.
Plain negative types permit only pure forcing; `/ {Use}` still describes
activation. Empty forcing rows can be omitted.

`let+ ready = pending` now forces an existing implicit delay without
applying the resulting function or demanding the resulting menu. Put that
binding inside the construction handler. Reusing `ready` does not repeat
construction; reusing `pending` still does. No value is memoized or mutated.

`lazy::Lazy<*T, E>` accepts either polarity and declares its demand row.
Use `Lazy<T, {Build}>` for an effectful `.force`; `Lazy<T>` is pure.
Negative-result thunks need no positive record wrapper. The new `<*T>`
mark means polarity-unrestricted, not a row parameter. `lazy::of_delayed`
and `lazy::to_delayed` adapt the explicit and implicit interfaces for
negative `T` without forcing during conversion.

`Stream` tails now explicitly allow delayed construction:
`Delayed<Stream<T, ..E>, ..E>`. Stream consumers and sequence bridges
accept that type too. Suspended effectful command blocks use
`Delayed<(;), ..E>` rather than conflating construction with activation
in `((;) / {..E})`. In exit groups `Delayed` already denotes a negative
computation, so no implicit dual is added.

`examples/delayed_and_lazy.sl` is a runnable migration example.

Before turning a `;` adapter, explicitly force an effectfully delayed
computation under its construction handler. Turning such a computation
directly is refused until adapters preserve its forcing boundary.

### Consumers do not return unit

A `select` arm must end in a command. Replace a returning sink such as
`select i64 { value => <value | println }` with
`fn(value: i64) { <value | println }` and use ordinary application without
a closing `>`. To keep a genuine consumer, end its arm by transferring to
an explicit continuation. Joints do not sequence returning sinks; delivery
to a non-returning component prevents delivery to subsequent components.

### `spawn`

`spawn` is removed entirely and has no replacement:

```sl
spawn { ... }
```

Concurrency-like process creation is not part of the λ̄μμ̃ core.

### `mu` declarations with value groups

`mu(values) | (continuations)` declared a consumer abstraction. A
declaration is a `command` (see "`mu` is the expression, `command` is the
declaration"), and `mu` is only the expression that captures the current
continuation, `mu T { k <= … }`.
