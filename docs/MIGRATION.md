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

path | read_file | (                          // new
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

fn k(return: -i32) <- Color {
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
