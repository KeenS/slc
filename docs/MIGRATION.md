# Migration Guide

This guide covers the syntax changes made during the λ̄μμ̃ redesign. Old forms
are rejected by the current compiler; each section shows the unsupported form
and its replacement.

## Lists

The builtin list type `[A]`, the list patterns `[p, …]`, and the
`list_new`/`list_len`/`list_push`/`list_get` builtins are gone. `List<T>` is
an ordinary recursive enum in the prelude, with `length`, `map`, `append`,
and the outcome-offering `command nth` beside it.

Unsupported:

```sl
fn f(xs: [+i64]) -> i64 { … }
let xs = list_push(list_new(), 10);
```

Write:

```sl
fn f(xs: List<i64>) -> i64 { … }
let xs = List::Cons(10, List::Nil);
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
select Colour { Red => 0 | out, Green => 1 | out }
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
mu Config { retries: out <= 3 | out, name: out <= "slant" | out }
mu Config { retries <= 3 | retries, name <= "slant" | name }
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
mu { k <= 42 | k }
mu i64 { k <= read_file(path, k, complain) }
```

## Function polarity

Every function now requires an arrow.

### Bare `fn`

Unsupported:

```sl
fn add(x: +i32, y: +i32) { x + y }
```

Write:

```sl
fn add(x: +i32, y: +i32) -> i32 { x + y }
```

### `+fn`

The polarity prefix is removed.

Unsupported:

```sl
+fn add(x: +i32, y: +i32) -> i32 { x + y }
```

Write:

```sl
fn add(x: +i32, y: +i32) -> i32 { x + y }
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
fn k(k1: -i32, k2: -i32) <- (i32, i32) { ... }
```

The comma-separated parameters before `<-` form the consumed continuation row.
The type after `<-` is the continuation produced by the function.

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
command route(x: +i32) | (k: -i32) { k(x) }
```

### `mu` is the expression, `command` is the declaration

For part of the redesign the declaration was spelled `mu`. It is `command`
again, and `mu` now names only the expression that captures the current
continuation — the two are different core constructs (`Λα. t`, a parameter the
caller supplies, against `μα. c`, the ambient continuation), and one keyword
for both hid that.

```sl
mu route(x: +i32) | (k: -i32) { x | k }        // old: the declaration
command route(x: +i32) | (k: -i32) { x | k }   // new

let source = mu(k) { read_file(path, k, err) };   // unchanged: the capture
```

A declaration written with `mu` is a parse error naming the difference.

A `mu` expression binds one thing, the continuation it captures, and writes it
in one group:

```sl
mu | (k: -String) { … }   // old
mu(k: -String) { … }      // new
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
command route(x: +i32) | (k: -i32) { k(x) }
```

### An empty parameter group is left out

A `command` writes only the groups it has.

```sl
command main() | (exit: -i32) { … }   // old
command main | (exit: i32) { … }      // new

command log(message: +String) | () { … }   // old
command log(message: +String) { … }        // new
```

The same applies to a local `mu`, which is usually the one with no values:
`mu(k) { … }`.

### Bottom annotation

The declaration denotes a command, so its result is bottom. The annotation is
optional and does not change lowering:

```sl
command route(x: +i32) | (k: -i32) -> ⊥ { k(x) }
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
(-I ⅋ O)
```

For a function-shaped positive type, use:

```sl
fn(I) -> O
```

For a negative function declaration, use:

```sl
fn(k: -O) <- I
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
v | k       // new

EXIT(0)     // old
0 | EXIT    // later — and `EXIT` itself is now gone, see below
```

`@` binds more loosely than every operator, so `a + b | k` sends the sum, and
it is not associative. The consumer may be any expression that produces one,
including a negative function applied to its row: `Color::Blue | code(answer)`.

Applying a function to a continuation is unchanged: it is ordinary call
syntax, because the callee's declared row already says which arguments are
consumers. `deliver(ok, err)` still reads as it did.

A cut has type `⊥`, so a branch that ends in one leaves the type of an `if` to
the other branch, and code after a cut in a block is unreachable.

## The top-level `EXIT` is gone

Ending the program is a right a helper is handed, never one it takes: `exit`
reaches helpers as a continuation parameter or inside a consumer built where
it is in scope.

```sl
fn die(m: +String) -> ⊥ { println(m); 1 | EXIT }        // old

command main | (exit: i32) {                             // new
    let die = select { m => { m | println; 1 | exit⟩ } };
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

command main | (exit: i32) {  // new
    "hi" | println;
    0 | exit⟩
}
```

The final value is no longer printed: a program's output is exactly what it
prints, and its status is what it sends to `exit`. A `main` that used to end
with a value must print it. Inside `main`, cut against the `exit` parameter
rather than the global `EXIT` — a `command` must consume the continuation it was
given, so a `main` that never reaches `exit` is a linearity error.

## `select` covers any positive type

`select` is no longer restricted to enums, its arms use the same patterns
`match` uses, and an arm is written `pattern <= command`:

```sl
select Color {                     // one arm per variant — the negative additive
    Red => 0 | return⟩,
    Green => 1 | return⟩,
}

select Reading {                   // one arm, binding every field — the negative multiplicative
    Reading { value, unit } => ((value | int_to_str) + unit) | out⟩,
}

select (+i64 ⊗ +i64) {             // a bare product names its type
    (left, right) => (left + right) | out⟩,
}
```

Two things changed. An arm's shape used to be a variant name with an optional
payload binder; it is now a pattern, so a struct or tuple shape is written the
way `match` writes it. And an arm used to be written the other way round, as
`command => pattern`:

```sl
select Color {
    0 | return => Red,     // old
    Red => 0 | return,     // new
}
```

The shape now comes first, where a `match` puts it, and `<=` points back at the
command — a `match` arm produces a value from a shape, and a `select` arm runs
a command when a shape arrives. Writing the old order is a parse error that
says so.

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

path | fs::read | (                            // new
    select String { content => ... }
    & select String { message => ... }
)⟩
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
(input, pos) | parse_value | (select i64 { value_end => ... } & report)⟩
```

A helper that only computes with values stays an ordinary positive `fn`.

## `|` is flow, `⟨⟩` is the cut, and `@` is gone

Everything moves left to right through one operator, and every step
composes. Brackets say where a chain is closed, and the stage beside a
bracket takes its role from that — so nothing has to be inferred:

```sl
v @ k                          // old
⟨v | k⟩                        // new — the cut

then(f, k)                     // old — the prelude combinator, now deleted
f | k                          // new — the same consumer, composed

21 @ then(double, out)         // old
⟨21 | double | out⟩            // new
```

`⟩` closes a chain: the stage before it consumes. A head that is not a
function needs no mark, so `v | f` applies and `v | k⟩` cuts; a head
that *is* a function composes, and `⟨` says it is the value flowing in
instead — `f | k⟩` composes into `k`, while `⟨f | k⟩` sends `f` to it. A
chain is flat, because composition is associative. `@` keeps its other
job, the as-pattern binder `c @ '0'..='9'`.

## A function is applied by flowing into it

`f(a)` is gone: an application is `a | f`, and several arguments are the
product they always were.

```sl
println(label(area(shape)))            // old
shape | area | label | println         // new

add(a, b)                              // old
(a, b) | add                           // new
```

Constructors still build — `Cons(h, t)` is unchanged. Everything else
flows, a `command` included: its values come from the chain and its exits
are the closing stage.

```sl
nth(xs, 2, found, missing)             // old
(xs, 2) | nth | (found & missing)⟩     // new
```

## Calls and rows are unary

A declaration binds one argument per group, so the parameters of a group
are its parts: `f(a, b)` is `f((a, b))`, and a continuation row is one
menu of exits. Nothing written changes, but a row is now a value — it can
be passed whole:

```sl
command nth(xs: List<T>, i: i64) | (found: T & missing: String)

(xs, 2) | nth | (found & missing)⟩           // the exits, as one menu
command forward(…) | (row: (T & String)) {   // or handed on unopened
    (xs, 2) | nth | row⟩
}
```

## A stdlib helper with values and continuations is a `command`

`traced` was a positive `fn` returning `-T`, so a caller built the consumer
and then cut into it. It takes values *and* continuations, which is what a
`command` is, so it is one — and the call reads like every other call:

```sl
42 | (("answer", out) | traced)⟩          // old
("answer", 42) | trace::tap | out⟩        // new
```

`defaulting(fallback, k)` is gone rather than converted. Its job was to
*be* a consumer in a row slot, and a command is not one; the slot takes
`select String { m => fallback | k⟩ }`, which is what the combinator was
hiding.

```sl
(xs, 9) | nth | (out & (0, out) | defaulting)⟩              // old
(xs, 9) | nth | (out & select String { m => 0 | out⟩ })⟩    // new
```

## Printing is an effect

`println` and `print` perform the `IO` effect the prelude declares, so a
declaration that prints carries `{IO}` in its row — `main` included. The
runtime installs the handler, so `main` may leave it undischarged and
nothing else may:

```sl
command main | (exit: i32) {                    // old
    "hi" | println;
    0 | exit⟩
}

command main | (exit: i32) / {IO} {             // new
    "hi" | println;
    0 | exit⟩
}

fn greet(name: String) -> Unit / {IO} { "hello, " + name | println }
```

A program can now handle its own output: a `handle` with a `write_line`
clause sits nearer the operation than the runtime's handler and answers
first. See `examples/io.sl`.

This bites where it did not before because a pipeline stage now charges
its effects at all: `x | throw` was silently free, and only the old call
form `throw(x)` was counted.

## `⟨` is never left out

A chain used to read its head as a value unless the head was a function, and
`⟨` was needed only to send a function on as a value. Now `⟨` always marks
what flows in, and a chain without it begins with a function, whatever its
head is:

```sl
"hi" | println;             // old: an application
⟨"hi" | println;            // new

0 | exit⟩                   // old: a cut
⟨0 | exit⟩                  // new

(a, b) | f                  // old: an application of `f`
⟨(a, b) | f                 // new

f | k⟩                      // composition into a consumer, as before
⟨f | k⟩                     // `f` itself sent to `k`, as before
```

A head that is not a function, written without `⟨`, is refused, pointing at
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
fn stop(k: -i32) -> (;) { 0 | k⟩ }                 // new

2 ⊗ 3                                              // old
2 * 3                                              // new
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
    match x { Outcome::Number(n) => n | int_to_str, Outcome::Text(s) => s }
}

fn show(x: (i64 | String)) -> String {                    // new
    match x { ::0(n) => n | int_to_str, ::1(s) => s }
}
```

A position is read against the sum its context gives, so `::1(v)` needs a
return type, an annotation, a parameter or a cut to say which sum it is in.

## A form value is written from its continuations

`(k1 ; k2)` is new: a value of `(T1 ; T2)` that, fed a product `(a, b)`, hands
`a` to `k1` and then `b` to `k2`. Nothing that compiled before changes.

## A `⅋` value fits the other spelling of its type

A slot declared at one spelling of `A ⅋ B` used to refuse a value written at
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
or command fewer values used to type-check — its `⅋`-nested type presented
the first parameter alone — and then crashed at run time with "a consumer
of 2 components received …". It is refused at check time now:

```sl
let inc = 1 | add;                    // was accepted, then crashed
(1, 2) | add                          // the call

"high" | route                        // refused: `route` takes (String ⊗ i64)
("high", 7) | route | k⟩              // the call, closing on its exits
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

(double, s) | seq::map                                   // new
```

## File operations are the `fs` module's

The six file builtins are no longer names a program has unasked. They are
the stdlib's `fs` module — commands over runtime primitives renamed
`__read_file` and so on, which a program is not meant to call:

```sl
path | read_file | (ok & failed)⟩              // old

path | fs::read | (ok & failed)⟩               // new
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
xs | length | println;
(3, 7) | num::min | println;                         // or by path, no import
("answer", 42) | trace::tap | out⟩
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

fn norm(p: Point) -> i64 { p.x * p.x + p.y * p.y }   // still fine
fn norm(Point { x, y }: Point) -> i64 { x * x + y * y }
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
throw(m) => 0 - 1                      // new: never resumes, no binder

config() resume => resume(10)          // old
config(): resume => resume(10)         // new: bound after the colon
```

## Effect rows: row variables, written like generics

A higher-order function forwards an argument's effects by declaring a
**row variable** — a generic parameter used with the `..` "rest" spelling
in a row, on its own arrow and on the parameter's:

```sl
fn map<A, B>(f: (A -> B), xs: List<A>) -> List<B>              // old: f had to be pure
fn map<A, B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}   // new
```

A bare arrow still means pure — now enforced against arguments too:
passing an effectful function where a rowless arrow is declared is an
error at the call site. `{Exn, ..E}` extends a variable; `main`'s row must
be empty.

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
Refutes(r) => 42 | r               // new

fn describe(note: ↓-String) -> ⊥ { "…" | ↑note }   // old
fn describe(note: -String) -> ⊥ { "…" | note }     // new
```

With no box to go through, `dual` is an involution on the nose: `-(-T)`
*is* `T`, and double-negation elimination is `fn dne<T>(t: -(-T)) -> T
{ t }`. The one rule that remains is orientation: the left of `@` is the
value side, so a continuation is passed as an argument, never cut against
data. Where a named box is still wanted, declare it — `menu Lazy<T>
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
        Red => 0 | return,
        Green => 1 | return,
        Blue => 2 | return,
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
        Parsed(text) => text | ok⟩,
        Failed(message) => message | err⟩,
    }
}
```

A pair of success and failure continuations threaded through every function
can therefore be replaced by one continuation that accepts a result enum.

### `choose`

Old variant-style `choose T { Variant }` syntax is removed.

Experimental struct-based `choose` implementation work has been removed while
its design is deferred. Do not migrate programs to any `choose` form; a future
design will be discussed separately.

## A `select` arm writes its shape first

An arm reads as a `match` arm does: the pattern, then what runs.

```sl
select T { command => pattern }     // old
select T { pattern => command }     // new
```

An arm that delivered to a continuation by calling it cuts instead:

```sl
select T { V => k(v) }              // old
select T { V => ⟨v | k⟩ }           // new
```

## A cut sends its value

`t | k` used to lower to a μ binder that shadowed `k`, so the value was sent
nowhere. The cut `⟨t | k⟩` now delivers `t` to `k`, as it always claimed to.

## `!` is the prelude's `not`

Logical not is an ordinary function a `bool` flows into.

```sl
!done                  // old
⟨done | not            // new
```

## Removed constructs

### `spawn`

`spawn` is removed entirely and has no replacement:

```sl
spawn { ... }
```

Concurrency-like process creation is not part of the λ̄μμ̃ core.

### Expression-level value-returning `mu`

The old expression-level/value-returning `mu` is removed. Use either a
positive function or the final `mu(values) | (continuations)` declaration,
according to the intended control behavior.

### `choose Struct`

`choose Struct` is removed, and no replacement exists.
