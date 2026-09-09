# Slant Language Design

Slant is a Rust-flavored programming language whose core semantics follow the
classical λ̄μμ̃ (lambda-bar-mu-mu-tilde) calculus. The surface language is
intentionally familiar to Rust programmers, but it does not pretend to be
literally symmetric: polarity is represented by types, arrows, and separate
binder groups rather than by inventing a second Rust-like syntax for
co-programs.

This asymmetry is intentional. The core calculus distinguishes terms,
co-terms, and commands, but a surface language with two parallel Rust-like
grammars would obscure both. Slant instead uses one familiar grammar and makes
polarity explicit through type signs and arrows.

## 1. Design goals

1. **Rust-like surface** — familiar `fn`, `mu`, `let`, `match`, braces, type
   annotations, and paths.
2. **λ̄μμ̃ core** — terms, co-terms, and cuts are the underlying semantic
   categories.
3. **Polarized types** — positive types denote values/proofs; negative types
   denote continuations/refutations.
4. **Explicit control** — a continuation is activated by a cut, `v @ k`,
   which is a command and not a call.
5. **Linear continuations** — continuation ports must be consumed exactly once
   on every terminating path.

## 2. Core model

```text
Program     ::= Command*
Command     ::= ⟨ Term ∥ CoTerm ⟩
Term        ::= value / proof
CoTerm      ::= continuation / refutation
Cut         ::= ⟨ Term ∥ CoTerm ⟩
```

Evaluation proceeds by cuts. A proof meets a refutation; the interaction
determines which reduction fires. There is no privileged application head and
no language-level concurrency primitive.

## 3. Application and cut

Two operations look alike in most languages and are different here.

**Application** `f(a)` supplies an argument to an abstraction and gets a
result: control returns. It works at either polarity, because the callee's
declaration already says what each argument slot takes. Supplying a
continuation to a negative function is therefore ordinary application —
nothing new is needed to apply a function to a continuation:

```sl
deliver(ok, err)        // `deliver` declares a row of two consumers
```

**A cut** `v @ k` sends the value `v` to the consumer `k`. It is the surface
spelling of the core's `⟨ v ∥ k ⟩`, and it is a *command*, not an expression
that happens to return: control does not come back, so nothing after it in a
block runs, and its type is `⊥`.

```sl
mu route(x: +i32) | (k: -i32) {
    x @ k
}
```

`@` binds more loosely than every operator, so `a + b @ k` sends the sum. It
is not associative: a cut has no result, so it cannot be the value of another
cut. The consumer may be any expression that produces one — a name, or a
negative function applied to its row:

```sl
Color::Blue @ code(answer)      // apply, then cut against the result
```

Because a cut has type `⊥`, a branch that ends in one constrains nothing: in
`if c { pos + 1 } else { message @ err }` the `if` has the type of the branch
that returns.

Calling a continuation is rejected. `k(v)` reports that `k` is a consumer and
not a function, because a reader — and the compiler — should not have to know
what `k` is bound to in order to tell an application from a command.

## 4. Function polarity

A positive function is the familiar value-to-value function:

```sl
fn add(x: +i32, y: +i32) -> i32 {
    x + y
}
```

A negative function consumes continuations and produces a continuation. It is
written with the reverse arrow:

```sl
enum Status { Ok(i64), Failed(i64) }

fn report(success: -i64, failure: -i64) <- Status {
    select Status {
        code @ success => Ok(code),
        code @ failure => Failed(code),
    }
}
```

The arrows identify the direction of the cut:

- `fn(value_params) -> Output` consumes values and produces a value.
- `fn(continuation_params) <- ContinuationType` consumes continuations and
  produces a continuation.

There is one `fn` declaration form. The old `+fn` and `-fn` prefixes do not
exist.

A negative function declaration is an ordinary negative abstraction. It does not
implicitly capture the current continuation. A distinct local `mu` expression
must be used when a body needs to capture control; its binders are independent
of the declared continuation parameters.

### Polarity by position

A type is positive or negative on its own; where it is written says which is
expected. There are four places to write one, and all four occur:

| | argument position | continuation position |
|---|---|---|
| **positive type** | data arrives: `fn f(x: +i64)` | the type after `<-`: `fn config() <- Request` |
| **negative type** | a consumer arrives: `fn f(note: -String)` | the row of a `mu`: `mu f() \| (k: -i64)` |

The diagonal is the ordinary reading — data in, control out. The other two are
what polarity buys:

- A **negative type in argument position** is a consumer received as data. A
  positive `fn` may take one, because it returns rather than ending in a cut,
  so it promises nothing about consuming it; a `mu` splits its parameters by
  polarity, so a consumer there belongs in the continuation group instead.
- A **positive type in continuation position** is the type after `<-`. A
  continuation is named by the type it consumes, so `fn config() <- Request`
  writes a positive type and produces its consumer.

Consuming codata reverses a cut's usual sides: a provider is negative, so what
consumes it is its dual — the positive request. In
`Request::Retries(answer) @ provider` the provider is the consumer and the
request is the value.

`examples/polarity.sl` writes all four; `examples/polarity_error.sl` writes
the two a `mu` rejects.

### Continuation rows

The continuation parameters of a negative function, and the second parameter
group of a `mu`, form that declaration's **continuation row**. A row is
compared **positionally and invariantly**:

- Two rows are equal when they have the same width and their positions are
  pairwise equal types, in the same order.
- There is no width subtyping: a row may not be widened with an extra
  continuation or narrowed by dropping one.
- There is no reordering: two rows that differ only in the order of their
  positions are different rows.
- There is no depth subtyping: a position accepts exactly its declared type.

The reason is linearity. Every continuation in a row is consumed exactly once
on every terminating path, so a row of a different width or order describes a
different linear behavior — not a compatible one.

What that obligation checks is that a continuation is not **dropped**. It does
not count occurrences: a cut does not return, so of several mentions of the
same continuation at most one can actually run, and the others are
unreachable. A parser may therefore forward its error consumer to a sub-parser
and also cut against it in the continuation that follows — exactly one of the
two runs. A *value* is linear in both directions, because nothing stops a
program from copying one: using a structural value twice is still an error. A call therefore supplies
each row position a continuation of exactly the declared type, and supplying
more arguments than the declaration has parameters is rejected.

An argument whose type the checker cannot determine — an unannotated `let`
binding, for instance — is not rejected; a row mismatch is reported only for
an argument whose type is known.

### Generic function parameters

A declaration may declare type parameters. In a positive function, a bare
generic parameter is positive; in a negative function or continuation row, a
bare generic parameter is negative. Thus `T` instantiates to the polarity
required by its position:

```sl
fn id<T>(value: T) -> T { value }
fn consume<T>(ok: -T) <- T { 0 @ ok }
```

An explicit sign is a constraint, not a change of representation. `+T` denotes
a positive instantiation and `-T` denotes a negative instantiation;
therefore `+T` is rejected in a continuation row, and `-T` is rejected for a
positive value parameter. Generic function declarations are type-erased at
lowering: their ordinary parameters lower to λ binders and their continuation
parameters lower to Λ co-abstraction binders.

## 5. `mu`: consumer abstraction

`mu` is the cut-oriented control declaration, and it is the form that takes
**both** values and continuations: value parameters and continuation
parameters appear in separate parenthesized groups, and the body is a command.
A program that consumes values and consumes a continuation is a `mu`; a
positive `fn` may still receive a consumer as ordinary data it forwards, but
it returns a value rather than ending in a cut.

```sl
mu route(x: +i32) | (k: -i32) {
    x @ k
}
```

The declaration denotes a command. Its return type is bottom; an optional
`-> ⊥` annotation may be used as documentation and does not change lowering.

Conceptually, `mu f(x: +A) | (k: -B) { E }` lowers to `λx. Λk. E`: the value
parameters bind first, so a call supplies arguments in the order the
parameters are written. Control leaves the body only by activating one of its
continuations.

As an *expression*, a `mu` with no value parameters captures the continuation
of the expression it stands in — this is the language's `call/cc`. Its value
is whatever that continuation receives, so `mu f() | (k: -A) { … }` has type
`A`, and a call whose result comes back through a continuation can be written
without nesting the rest of the program inside it:

```sl
let source = mu here() | (k: -String) {
    read_file(path, k, complain)
};
print(source);
```

`k` is the continuation of the `let`: what `read_file` sends it becomes
`source`, and the block continues. On the other outcome `k` is never
activated, so nothing after the `let` runs.

This is how a fallible operation is written. Rather than returning a result
that a caller inspects, it takes the continuations its outcomes belong to:

```sl
mu parse_value(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) {
    match at(input, pos) {
        QUOTE => parse_string(input, pos, ok, report),
        _ => ParseResult::Failed("expected JSON value") @ report,
    }
}
```

Each path ends in a cut: either forwarding both continuations to another
command, or sending an outcome to one of them. A helper that only computes
with values — `at` above — stays an ordinary positive `fn`.

## 6. Additive data

### Positive additive construction

An `enum` is a positive additive sum. A value contains exactly one variant:

```sl
enum Color { Red, Green, Blue }

enum Shape {
    Point,
    Circle(i64),
    Rect(i64, i64),
}
```

A variant may carry a payload. A variant with no payload is a value of the
declaration; a variant with a payload is a constructor from that payload to
it, so `Shape::Point` is a `Shape` while `Shape::Circle` is only a `Shape`
once it is applied to an `i64`. Several payload values are packed into one
right-nested tensor, so every variant carries exactly one payload.

`match` decomposes an enum by choosing the corresponding arm, and a variant
pattern binds exactly the payload its variant declares:

```sl
match shape {
    Point => 0,
    Circle(r) => 3 * r * r,
    Rect(w, h) => w * h,
}
```

### Negative additive construction

`select` builds the consumer of a positive type by giving, for each shape that
type can take, a command. For an `enum` that is one arm per variant — the
negative additive:

```sl
fn k(return: -i32) <- Color {
    select Color {
        0 @ return => Red,
        1 @ return => Green,
        2 @ return => Blue,
    }
}
```

The negative function receives the consumer continuation `return`; activating
the constructed continuation with an enum value dispatches to the matching arm,
which activates that arm's consumer. Arms must cover exactly one enum variant
each, must be exhaustive, and must not repeat variants.

An arm may bind the payload of its variant and pass it to the consumer, which
is how a value reaches the continuation the variant selects:

```sl
enum ParseResult { Parsed(String), Failed(String) }

fn deliver(ok: -String, err: -String) <- ParseResult {
    select ParseResult {
        text @ ok => Parsed(text),
        message @ err => Failed(message),
    }
}
```

`Parsed(text)` binds the payload of `Parsed` as `text` for that arm only. A
variant that carries a payload must bind it; a variant that carries none must
not.

An arm's left-hand side must be a cut `v @ k` whose consumer is a visible
negative binding. The arm lowers to that command, so a `select` expression is
a genuine negative additive
consumer — one branch per variant — and not an opaque builtin. Activation
chooses exactly one branch: the branches of the arms that were not selected
are never evaluated, neither when the consumer is constructed nor when it is
activated.

`return` is a reserved identifier, but it is accepted as an ordinary
continuation parameter name. It may then be used as an expression callee, as an
expression argument, and as a struct-name marker in ordinary call syntax.

## 7. Multiplicative data

### Positive multiplicative construction

A `struct` is a positive product. A value contains all fields:

```sl
struct Direction { left: i32, right: i32 }
```

Declaration names have opaque core types (`Named`). Core unification treats two
named types as equal only when their names match; the surface checker owns
field presence, field types, order, and exhaustiveness. This keeps named
declarations distinct from the tensor unit `1`.

A struct declaration's *representation* is the right-nested tensor of its
field types — `+i32 ⊗ +i32` for the declaration above — and a struct with no
fields is the tensor unit `1`. A struct literal and a struct pattern must both
write every declared field exactly once, in declaration order, with the
declared type.

A struct *value* is that tensor labelled by the declaration's name, exactly as
an `enum` variant is its payload labelled by the variant's name. One core form
covers both, which is why one surface form — `match` — takes either apart.

A `struct` value carries every field at once, which is what `⊗` means: the
positive product of its field types, associated to the right. `struct
Direction { left: i32, right: i32 }` describes `+i32 ⊗ +i32`, and the surface
tuple `(a, b)` is the same connective written anonymously.

### Explicit connective types

Both multiplicative connectives are available as explicit *type* syntax, and
are always parenthesized:

```sl
fn sum_pair(p: (+i64 ⊗ +i64)) -> i64 { … }
mu consume_pair() | (k: (-i64 ⅋ -i64)) { … }
```

| Type syntax | Meaning |
|---|---|
| `(A ⊗ B)` | positive product; the anonymous form of a two-field `struct` |
| `(A ⅋ B)` | negative product; the dual of `⊗`, a joint consumer of both sides |
| `(A -> B)` | function — and `(A -> ⊥)` *is* `-A`, since a function that never returns is a consumer of its argument |
| `[A]` | list |
| `dual(A)` | the dual of `A`, applied — `dual(+i64)` *is* `-i64`, and `dual(dual(A))` is `A`. Only a declaration's name stays wrapped, since it is opaque to the core |
| `⊥` | bottom |

`⊗` and `struct` are the same connective: a `struct` declaration names a
product and its fields, while `(A ⊗ B)` writes one anonymously. Neither is
sugar for the other — a named declaration is opaque to core unification, while
an explicit tensor is structural.

### Negative multiplicative construction

The consumer of a product is built the same way, by `select`. A product has
exactly one shape, so it has exactly one arm, and that arm binds every
component — which is what makes it multiplicative rather than additive: the
halves arrive together, in one command, sharing its context.

```sl
struct Reading { value: i64, unit: String }

// dual(Reading) is `-i64 ⅋ -String`: one consumer with both halves
fn show(out: -String) <- Reading {
    select Reading {
        (int_to_str(value) + unit) @ out => Reading { value, unit },
    }
}
```

A bare product needs no declaration; its shape is written as the type:

```sl
fn total(out: -i64) <- (+i64 ⊗ +i64) {
    select (+i64 ⊗ +i64) {
        (left + right) @ out => (left, right),
    }
}
```

Either is consumed by the cut that supplies the whole product:

```sl
Reading { value: 42, unit: "m" } @ show(out)
(2, 40) @ total(out)
```

There is deliberately **no** way to build such a consumer out of two
independent consumers, and no way to feed one half at a time. Both are the
same thing — halves that progress independently — and both need either a send
that returns or concurrency. A cut does not return, and the language has no
concurrency, so a `⅋` is supplied whole.

## 8. Top-level exit

`EXIT` is the top-level continuation. Its type is `-i32`, so it is activated
by a cut like any other consumer:

```sl
0 @ EXIT
```

The cut terminates the program with the supplied exit code. It is the same
continuation the runtime hands to `main`, so `0 @ exit` inside `main` and
`0 @ EXIT` anywhere else end the program the same way. Inside `main`, use the
parameter: a `mu` must consume the continuation it was given.

## Entry point

A program is a command, so its entry point is a `mu`. It takes no values and
exactly one continuation — the exit status:

```sl
mu main() | (exit: -i32) {
    println("Hello, Slant!");
    0 @ exit
}
```

The runtime supplies that continuation, and it is `EXIT`: the two names denote
the same thing, so a helper that ends the program can cut against `EXIT`
directly while `main` uses the name it was given. The cut that reaches it is
what ends the program, and the integer it carries is the process exit status.

Because a `mu` must consume its continuation, **every terminating path of a
program leaves through `exit`** — a `main` that falls off the end is rejected
by the linearity check, not by a runtime convention.

There is no final-result value. A program's output is exactly what it prints;
its status is what it sends to `exit`. A `fn main`, a `main` with value
parameters, a `main` whose row is not one exit status, and a missing `main`
are all rejected.

The evaluator runs on a dedicated stack, so how deeply a continuation-passing
program nests is bounded by memory rather than by the host's default stack.

## Standard library

The library follows the same rule the language does: **a builtin whose outcome
is a single value is an ordinary function; a builtin whose outcome is not —
it can fail, or find nothing — takes continuations and denotes a command.**
The value arguments come first, then one continuation per outcome, and exactly
one of them is activated.

| Builtin      | Values                                           | Outcomes                                            |
|--------------|--------------------------------------------------|-----------------------------------------------------|
| `parse_int`  | `text: +String`                                  | `ok: -i64`, `invalid: -String`, `overflow: -String` |
| `read_file`  | `path: +String`                                  | `ok: -String`, `failed: -String`                    |
| `write_file` | `path: +String`, `contents: +String`             | `ok: -unit`, `failed: -String`                      |
| `char_at`    | `text: +String`, `index: +i64`                   | `ok: -char`, `out_of_range: -String`                |
| `list_get`   | `list`, `index: +i64`                            | `ok`, `out_of_range: -String`                       |
| `map_get`    | `map`, `key`                                     | `found`, `missing: -String`                         |
| `find_char`  | `text: +String`, `from: +i64`, `character: +i64` | `found: -i64`, `absent: -String`                    |

Every failure continuation receives a `+String` describing what happened, so
it composes with an error consumer a program already has.

```sl
read_file("input.json", fn(source: +String) -> ⊥ {
    parse_json(source, deliver(ok, err))
}, fn(message: +String) -> ⊥ {
    message @ err
})
```

Everything else is a function: `println`, `print`, and `format`; arithmetic and
comparison; `str_len`, `str_concat`, `int_to_str`, `str_eq`, `substring`;
`is_digit`, `is_ws`, `skip_ws`, `skip_digits`; `file_exists`; and the
`list_`/`map_`/`set_`/`path_` constructors and totals.

Two failures stay fatal rather than becoming outcomes: an out-of-range index
`s[i]` and a division by zero. They are reached through operator syntax, which
has nowhere to put a continuation, and — as in Rust, where `v[i]` panics while
`v.get(i)` does not — they report a bug in the program rather than a case it
was meant to handle. The checked forms are the `mu`-shaped builtins above.

A helper of your own that always ends in a cut is annotated `-> ⊥`: it never
returns, so it may stand where a consumer is expected.

## Literals

A lambda whose body ends in a cut produces nothing, so it *is* a consumer:
`fn(message: +String) -> ⊥ { … }` has type `-String`, and may be written
wherever a consumer of a `String` is expected. `A → ⊥` and `-A` are the same
type, not two that convert, so a continuation parameter may be annotated
either way.

The `-> ⊥` may be omitted — the body decides the type — but the examples
write it, because a consumer literal is worth reading as one at a glance.

An integer literal takes the integer type its port requires — `0 @ EXIT`
sends an `i32` — and is `+i64` when nothing constrains it. Every other value
must match its port exactly: there is no implicit widening or narrowing of a
value that is not a literal.

A floating-point literal is untyped for now, a string literal is `+String`, a
character literal is `+char`, and `true` and `false` are `+bool`. `()` is the
unit value, of type `1`; an empty block is the same.

## Diagnostics

Compiler failures are categorized by the phase that produces them:

| Category         | Meaning                                                                              |
|------------------|--------------------------------------------------------------------------------------|
| `parse`          | the source is not a valid surface program                                            |
| `type`           | a term has the wrong type or an inference rule cannot apply                          |
| `polarity`       | a value or continuation is used with the wrong polarity                              |
| `linearity`      | a linear variable or continuation is not used exactly once on every terminating path |
| `exhaustiveness` | a `match` or `select` does not cover its alternatives exactly once                   |
| `lowering`       | an otherwise accepted surface construct cannot be translated to the core calculus    |

The compiler applies these phases in order:

1. `parse`
2. `type`
3. `polarity`
4. `linearity`
5. `exhaustiveness`
6. `lowering`

A phase stops before later phases once it reports a diagnostic. Consequently,
`parse` diagnostics take precedence over all checker diagnostics; `type`
diagnostics take precedence over polarity, linearity, and exhaustiveness; and
so on. Within one phase, diagnostics are source-ordered. Checker diagnostics
include `line:column` positions and source excerpts.

Runtime failures are not compiler diagnostics. They are reported after
evaluation begins and do not participate in this precedence order.

## 9. Core calculus

### Grammar

```text
Term      t ::= x                     variable
              | λx. t                 value abstraction
              | μα. c                 capture of the ambient continuation
              | Λα. t                 continuation abstraction (negative function)
              | t ⊗ t                 tensor pair
              | inl(t) | inr(t)       binary additive injection
              | L(t)                  labelled additive injection (enum value)
              | co(e)                 a co-term reified as a negative value

CoTerm    e ::= α                     co-variable
              | λ̄x. c                 co-abstraction (application)
              | μ̃x. c                 value abstraction
              | e ⅋ e                 par
              | fst | snd             tensor projections
              | μ̃[L₁(x…). c₁ | … ]    labelled consumer (enum, struct)
              | μ̃(x₁, …, xₙ). c       product consumer

Command   c ::= ⟨ t ∥ e ⟩             cut
              | κx. t                 command abstraction
              | k(v)                  continuation activation

Type      A ::= +B | -B               positive / negative atom
              | A ⊗ A | A ⅋ A         multiplicatives
              | 1 | ⊥                 their units
              | A + A | A & A         additives
              | !A | [A] | A → A      exponential, list, function
              | dual(A) | Named | ?v  dual, declaration name, inference variable
```

`Λα. t` and `μα. c` both bind a continuation variable, and they are not
interchangeable. `Λα. t` is a *declared* continuation parameter: the caller
supplies the continuation. `μα. c` captures the *ambient* continuation: that is
what a local `mu` expression does. The surface keeps them apart by where they
are written, so lowering never has to guess.

### Printed form

The grammar above is also the core's printed form: the compiler prints terms,
co-terms, commands, and types in exactly this syntax, and reads them back
unchanged. Printing is therefore a faithful view of the IR rather than an
approximation of it, and a printed declaration can be compared, stored, or
re-parsed without loss.

### Reduction

```text
⟨ λx. t ∥ μ̃y. c ⟩            → c[t/x]              β
⟨ μα. c ∥ e ⟩                → c[e/α]              μ
⟨ t ∥ λ̄x. c ⟩                → c[t/x]              co-β
⟨ t₁ ⊗ t₂ ∥ fst ⟩            → t₁                  projection
⟨ t₁ ⊗ t₂ ∥ snd ⟩            → t₂                  projection
⟨ L(v₁ ⊗ …) ∥ μ̃[… L(x…). c …] ⟩ → c[vᵢ/xᵢ]          labelled
⟨ v₁ ⊗ v₂ ∥ μ̃(x, y). c ⟩     → c[v₁/x, v₂/y]      product
```

The labelled rule is what makes `select` lazy: the label of the value selects
one branch, and the branches that were not selected are discarded unreduced.
An `enum` has a branch per variant and a `struct` exactly one, so the same
rule covers the additive and the labelled multiplicative; the product rule is
its unlabelled counterpart.

Applying a continuation abstraction instantiates its parameter with the
supplied continuation rather than with the ambient one:

```text
⟨ Λα. t ∥ λ̄x. ⟨ v ∥ β ⟩ ⟩    → ⟨ t[v/α] ∥ β ⟩       continuation instantiation
```

### Lowering table

Every accepted surface construct lowers as follows. `⟦e⟧` is the lowering of
`e`, and `f(a)` abbreviates the application encoding `μ__call. ⟨ ⟦f⟧ ∥ λ̄__f.
⟨ ⟦a⟧ ∥ __call ⟩ ⟩`, nested left to right for several arguments.

| Construct | Surface | Core |
|---|---|---|
| `expr.literal` | `42`, `"s"`, `'c'`, `true` | a constant variable (`$int_42`, `$str_"s"`, …) |
| `expr.ident` | `x` | `x` |
| `expr.enum` | `Color::Red`, `Shape::Circle(r)` | `Color::Red(unit)`, `Shape::Circle(⟦r⟧)` — several payload values pack into one tensor |
| `expr.call` | `f(a, b)` | `f(a)(b)` (curried application encoding) |
| `expr.lambda` | `fn(x: +A) -> B { e }` | `λx. ⟦e⟧` |
| `expr.pair` | `(a, b)`, `()` | `⟦a⟧ ⊗ ⟦b⟧`, right-nested; `()` is `unit` |
| `expr.let` | `let x = v; e` | `μlet. ⟨ ⟦v⟧ ∥ λ̄x. ⟨ ⟦e⟧ ∥ let ⟩ ⟩` |
| `expr.block` | `{ e₁; e₂ }` | `μ__seqᵢ. ⟨ ⟦e₁⟧ ∥ λ̄__discarded. ⟨ ⟦e₂⟧ ∥ __retᵢ ⟩ ⟩` |
| `expr.if` | `if c { t } else { e }` | `__if_dispatch(⟦c⟧, λ_. ⟦t⟧, λ_. ⟦e⟧)` — branches are thunks, so only the chosen one runs |
| `expr.binop` | `a + b` | `add(⟦a⟧)(⟦b⟧)`; `&&` and `\|\|` expand to `expr.if` first, keeping them short-circuiting |
| `expr.unop` | `-a`, `!a` | `neg(⟦a⟧)`, `eq(⟦a⟧)(false)` |
| `expr.index` | `a[i]` | `__index(⟦a⟧)(⟦i⟧)` |
| `expr.slice` | `a[i..j]` | `substring(⟦a⟧)(⟦i⟧)(⟦j⟧)` |
| `expr.cut` | `v @ k` | `μ__cut. ⟨ ⟦v⟧ ∥ k ⟩` for a named consumer, and `μ__cut. ⟦k⟧(⟦v⟧)` for a computed one. The μ binder is never referenced — a command has no result — and is renamed if the consumer is called `__cut` |
| `expr.mu` | `mu f() \| (k: -A) { e }` | `Λ`-free: `μk. ⟨ ⟦e⟧ ∥ k ⟩` — the local form captures the ambient continuation |
| `expr.match` | `match s { p => e, … }` | `__match_dispatch(⟦s⟧, arm₁, …)`; each arm is `inl(descriptor ⊗ (guard ⊗ λ__match_arg. ⟦e⟧))`, so an arm body runs only when its pattern matches |
| `expr.struct` | `S { f: v, g: w }` | `S(⟦v⟧ ⊗ ⟦w⟧)` — the declaration's name labelling the right-nested tensor of its fields, the same shape a variant has |
| `expr.select` | `select T { c => p, … }` | `co(μ̃[ L(x…). ⟦c⟧ … ])` for a labelled type — one branch per shape, the pattern's binders naming that shape's components — and `co(μ̃(x…). ⟦c⟧)` for a product |
| `expr.errorprop.named` | `e?k` | `⟦e⟧(k)` |
| `expr.errorprop.bare` | `e?` | `⟦e⟧(k₀)`, where `k₀` is the current error continuation |
| `decl.fn.positive` | `fn f(x: +A) -> B { e }` | `λx. ⟦e⟧` |
| `decl.fn.negative` | `fn f(k: -A) <- B { e }` | `Λk. ⟦e⟧` |
| `decl.mu` | `mu f(x: +A) \| (k: -B) { e }` | `λx. Λk. ⟦e⟧` |
| `decl.const` | `const C: +A = v;` | `⟦v⟧` |
| `decl.enum` | `enum E { V }` | one global per variant: `E::V = E::V(unit)` |
| `decl.struct` | `struct S { … }` | no term; the declaration is a type |

Binders are nested in declaration order, so a call supplies arguments in the
order the parameters are written; a value parameter becomes a λ binder and a
continuation parameter becomes a Λ binder.

### Surface-to-core coverage

| Core construct | Surface representation |
|---|---|
| `x`, `λx. t` | identifiers, positive functions, lambdas |
| `μα. c` | local `mu` expression, `@` against a named consumer, and the lowering of `let`, blocks, and applications |
| `Λα. t` | a declared continuation parameter of `fn … <- …` or of a `mu` |
| `t ⊗ t` | tuple literals, `struct` literals, `(A ⊗ B)` values |
| `L(t)` | `enum` values and `struct` values — a labelled product |
| `inl` / `inr` | not surface-visible; used internally to tag lowered `match` arms |
| `co(e)` | `select` |
| `α` | the consumer named on the right of a cut, `v @ k` |
| `λ̄x. c` | application |
| `μ̃x. c` | not surface-visible; produced by reduction |
| `μ̃[…]` | `select` over an `enum` or a `struct` |
| `μ̃(x…)` | `select` over a bare product |
| `e ⅋ e` | not surface-visible: a `⅋` consumer is built by `select` over a product |
| `fst` / `snd` | not surface-visible; `match` destructuring recovers components |
| `κx. t` | not surface-visible; the internal command abstraction |
| `k(v)` | a cut whose consumer is computed rather than named: `v @ f(a)` |

## 10. Error continuations

Fallible operations receive their result continuations directly. For example,
a parse operation receives both a success continuation and an error
continuation:

```sl
parse_json(source, deliver(ok, err))
```

Success flows to `ok`; failure flows to `err`. No result wrapper is needed:
`deliver` builds one consumer from the two with `select`, and each parser
sends its outcome to it with a cut.

### Error propagation `?`

The expression form `e?` remains supported as explicit selected-continuation
sugar. It does not hide control flow behind a value: it names which
continuation receives the failure.

- `e?k` supplies the named continuation `k` to `e`. It lowers to the
  application `⟦e⟧(k)` — the same core term as writing the call by hand.
- `e?` supplies the **current error continuation**: the last continuation
  declared by the innermost enclosing row. Lexical scope decides this, so a
  nested local `mu` shadows the enclosing row inside its own body, and a
  lambda body has no row at all — a bare `?` inside one is rejected rather
  than silently reaching outward.
- A bare `?` with no continuation in scope is rejected.

A name written **immediately** after `?`, with nothing between them, is the
selected continuation. Any name the language accepts as a continuation
parameter is accepted there, including the reserved word `return`. With a
space — `e? name` — the `?` is bare and `name` is a separate expression.

## 11. Migration summary

- `k(v)` (activating a continuation) → `v @ k`
- `EXIT(0)` → `0 @ EXIT`
- `select T { k(v) => V }` → `select T { v @ k => V }`
- `t @ k` previously lowered to a μ binder that shadowed `k`, so it sent the
  value nowhere; it is now the cut it always claimed to be
- `command name(...)` → `mu name(...) | (...)`
- `spawn` → removed; no replacement exists
- `mu(x, to k)` → `mu(x) | (k)`
- `agent.to(k, h)` and `agent.consume(k, h)` → removed; no partial-agent replacement exists
- `+fn` → `fn ... -> ...`
- `-fn` → `fn ... <- ...`
- bare `fn` → rejected; write either `->` or `<-`
- old variant-style `choose T { Variant }` → removed
- `choose Struct` → removed while its design is deferred
