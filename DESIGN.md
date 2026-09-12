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

What the one grammar does keep is the mirror. A program can be written
value-first — functions take data and give data back — or continuation-first,
where a function takes a consumer and gives a consumer back and nothing
returns at all. Each construct has its opposite: `fn f(x: +A) -> B` against
`fn f(k: -B) <- A`, `match` against `select`, a call against a cut, and a
`let` against the consumer that the rest of the program becomes.
`examples/two_styles.sl` writes one program both ways.

## 1. Design goals

1. **Rust-like surface** — familiar `fn`, `command`, `let`, `match`, braces, type
   annotations, and paths.
2. **λ̄μμ̃ core** — terms, co-terms, and cuts are the underlying semantic
   categories.
3. **Polarized types** — positive types denote values/proofs; negative types
   denote continuations/refutations.
4. **Explicit control** — a continuation is activated by a cut, `v @ k`,
   which is a command and not a call.
5. **Total control** — every terminating path reaches a continuation: a
   `command` body must be `⊥`. The core is classical, so *which* continuation
   (and how many times) is up to the program.

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
command route(x: +i32) | (k: -i32) {
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
        Ok(code) => code @ success,
        Failed(code) => code @ failure,
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
| **negative type** | a consumer arrives: `fn f(note: -String)` | the row of a `command`: `command f \| (k: -i64)` |

The diagonal is the ordinary reading — data in, control out. The other two are
what polarity buys:

- A **negative type in argument position** is a consumer received as data. A
  positive `fn` may take one, because it returns rather than ending in a cut,
  so it promises nothing about consuming it; a `command` splits its parameters by
  polarity, so a consumer there belongs in the continuation group instead.
- A **positive type in continuation position** is the type after `<-`. A
  continuation is named by the type it consumes, so `fn config() <- Request`
  writes a positive type and produces its consumer.

Consuming codata reverses a cut's usual sides: a provider is negative, so what
consumes it is its dual — the positive request. In
`Request::Retries(answer) @ provider` the provider is the consumer and the
request is the value.

`examples/polarity.sl` writes all four; `examples/polarity_error.sl` writes
the two a `command` rejects.

### Continuation rows

The continuation parameters of a negative function, and the second parameter
group of a `command`, form that declaration's **continuation row**. A row is
compared **positionally and invariantly**:

- Two rows are equal when they have the same width and their positions are
  pairwise equal types, in the same order.
- There is no width subtyping: a row may not be widened with an extra
  continuation or narrowed by dropping one.
- There is no reordering: two rows that differ only in the order of their
  positions are different rows.
- There is no depth subtyping: a position accepts exactly its declared type.

The reason is the calling convention, not linearity. A row is a fixed
positional interface: a caller supplies exactly one continuation per position,
of exactly the declared type, so a wider, narrower, or reordered row is a
different interface — not a compatible one.

The core is **classical**, so continuations are not linear. A row position may
go unused — a `command` that reaches one continuation and ignores the rest is
well-formed — and a continuation may be mentioned several times, since a cut
does not return, so at most one actually runs (a parser can forward its error
consumer to a sub-parser *and* cut against it in the continuation that
follows). Data is unrestricted too: values are freely copied and dropped.

What *is* enforced is that control is **total**: a `command` body must be `⊥`
— it reaches a continuation on every path — so a body that falls off the end
(a bare value) or dangles (an `if` with no `else`, whose false path yields
unit) is rejected by the type checker, not by any linearity pass. A call
supplies each row position a continuation of exactly the declared type, and
supplying more arguments than the declaration has parameters is rejected.

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
parameters lower to λ binders as well — a continuation is a value like any other.

## 5. `command`: consumer abstraction

A `command` declaration is the form that takes **both** values and
continuations: value parameters and continuation parameters appear in separate
parenthesized groups, and the body is a command — hence the name. A
declaration that consumes values and consumes a continuation is a `command`; a
positive `fn` may still receive a consumer as a value it forwards — `-String`
is a value type like any other — but it returns rather than ending in a cut.

```sl
command route(x: +i32) | (k: -i32) {
    x @ k
}
```

The declaration denotes a command. Its return type is bottom; an optional
`-> ⊥` annotation may be used as documentation and does not change lowering.

A group with nothing in it is left out rather than written empty: `command
main | (exit: -i32)` takes no values, and `command log(message: +String)` takes
no continuations. An empty `()` is a parse error saying so.

Conceptually, `command f(x: +A) | (k: -B) { E }` lowers to `λx. λk. E`: the
value parameters bind first, so a call supplies arguments in the order the
parameters are written. Control leaves the body only by activating one of its
continuations. `k` is a *parameter*: the caller passes it.

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
command parse_value(input: +String, pos: +i64) | (ok: -i64, failed: -String) {
    match at(input, pos) {
        QUOTE => parse_string(input, pos, ok, failed),
        _ => "expected JSON value" @ failed,
    }
}
```

Each path ends in a cut: either forwarding both continuations to another
command, or sending an outcome to one of them. One continuation per outcome
*is* the outcome type — see §11. A helper that only computes
with values — `at` above — stays an ordinary positive `fn`.

## 7. Additive data

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

`select` builds the consumer of any positive type by giving, for each shape
that type can take, a command. For an `enum` that is one arm per variant — the
negative additive:

```sl
fn k(return: -i32) <- Color {
    select Color {
        Red => 0 @ return,
        Green => 1 @ return,
        Blue => 2 @ return,
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
enum Reading { Measured(i64), Missing }

fn report(value: -i64, absent: -i64) <- Reading {
    select Reading {
        Measured(measurement) => measurement @ value,
        Missing => -1 @ absent,
    }
}
```

`Measured(measurement)` binds the payload of `Measured` for that arm only. A
variant that carries a payload must bind it; a variant that carries none must
not. A component that is itself a product — a tuple or a record — may be
taken apart in place, `Rect((w, h))` or `At(Point { x, y }, d)`: a product
has one shape, so deeper destructuring keeps the one-arm-per-shape law. A
*sum* inside a component cannot be split across arms, and there are no
guards or literal arms: a branch table answers each shape exactly once,
unordered, while guards and literals make the arm list ordered, first-match
— that is `match`, inside the arm.

An arm writes `=>`, exactly as a `match` arm does, because the arrow marks
which side of the mirror the scrutinee is on: **data flows forward into an
arm (`=>`); a demand reaches back into it (`<=`)**. A `select` matches data,
so its arms are `pattern => command`; a `mu` answers demands, so its arms are
`copattern <= command`; and a `match` writes whichever its scrutinee calls
for — `p => e` over a value, `.item(out) <= e` over a continuation. The
command an arm runs must be a cut `v @ k` whose consumer is a visible
negative binding. The arm lowers to it, so a `select` expression is a genuine negative
additive consumer — one branch per variant — and not an opaque builtin. Activation
chooses exactly one branch: the branches of the arms that were not selected
are never evaluated, neither when the consumer is constructed nor when it is
activated.

`return` is a reserved identifier, but it is accepted as an ordinary
continuation parameter name. It may then be used as an expression callee, as an
expression argument, and as a record-name marker in ordinary call syntax.

### `menu`: the negative additive declared

`menu` declares the negative additive type itself — the mirror of `enum`. An
enum value is one tagged variant the producer chose; a menu value answers one
item the consumer demands:

```sl
enum Config { Retries(-i64), Name(-String) }   // ⊕ — the value picks
menu Config { retries: i64,  name: String }    // &  — the demand picks
```

The nullary case is the additive unit `⊤`: `menu Top {}` has no possible
requests, and its unique menu value is `mu Top {}`. Its core form is
`μ[Top]`; the name is retained explicitly because an empty branch table has
no destructor label from which its nominal type could be recovered.

All four nullary connective shapes are introduced by prelude declarations:

```sl
data Unit {}
form Bottom {}
enum Empty {}
menu Top {}
```

The exact nullary `Unit` and `Bottom` declarations are recognized structurally:
`Unit` is the surface name of core `1`, constructed interchangeably as `()` or
`Unit {}`, and `Bottom` is the surface name of core `⊥`; its nullary demand
`Bottom {}` is likewise `()`. A differently shaped declaration that shadows
either name stays nominal. `select Empty {}` is the empty consumer of the
uninhabited `Empty`, and `mu Top {}` is the unique `Top` value. Concise surface
aliases for `Empty` and `Top` are deliberately deferred.

The two declarations above are each other's dual: `dual(i64 & String)` is
`-i64 ⊕ -String`, a sum of requests each carrying the continuation that wants
the answer — the shape the enum writes with explicit boxes. `menu` makes that
type native, so the boxes and the encoding disappear.

Each keyword owns one core family, so the branch tables split by what
arrives at them:

- **`select` answers data** — it builds the μ̃ family: the consumer of an
  atom, a product, an enum, or the record a `form` consumes. One arm per
  shape the data can take.
- A `mu` arm is `item: out <= c`: it binds the demand's continuation — its
  *return address*, carried the way a variant's payload is — and answers it
  with a command. The continuation can receive a direct answer with
  `item <= value @ item`, or route control through nested copatterns and
  multi-outcome builtins.

- **`mu` answers demands** — it builds the μ family: a binder arm `mu { k <= c }` captures
  the ambient continuation, and `mu Config { … }` is the copattern form, a
  menu value. An arm is `item: out <= c`: the destructor it answers, the
  binder for the continuation the request carries (`out: -A` for an item
  answering `A`), and the command that answers it. Arms cover each item
  exactly once, only the demanded branch ever runs, and the menu's name may
  be left out when a labelled copattern names it unambiguously. `item <= c`
  is shorthand for `item: item <= c`. An arm may *refine*
  an item whose answer is itself a menu with a nested copattern —
  `tail: head: out` — and the arms sharing an outer destructor group into
  an inner menu, which must again cover every item
  (`examples/stream.sl`).
- **`match` — a branch table applied to a named scrutinee, on either side.**
  Over an enum value it takes data apart; over a continuation of a menu type
  (`k: -Config`) it takes the *request* apart: `.item(out) => e` binds the
  request's own continuation, and the arms are ordinary expressions —
  typically other requests.

```sl
fn config() -> Config {
    mu Config {
        retries: out <= 3 @ out,
        name: out <= "slant" @ out,
    }
}

fn reroute(k: -Config) -> -Config {
    match k {
        .retries(out) <= .retries(out),
        .name(out) <= .name(out),
    }
}
```

Three request forms complete the surface:

- `cfg.item` **demands** one item off a menu — the mirror of record
  projection, and typed as the item's answer. Only that branch runs.
- `.item(k)` is a **request literal** — the mirror of an enum variant
  expression: a variant is data the producer tags, a request is a demand the
  consumer tags. It has type `-Config`, and `k` must consume the answer.
- `v @ request` **cuts** a menu against a request directly; `mu` names where
  the answer goes.

A menu type is negative, but a menu is a *value*: it may be returned
(`-> Config`), passed as a value parameter, and sit on the left of `@` — the
box discipline applies to consumers, and a menu is the thing consumers'
requests are sent to, not a consumer.

## 8. Multiplicative data

### Positive multiplicative construction

A `data` is a positive product. A value contains all fields:

```sl
data Direction { left: i32, right: i32 }
```

Declaration names have opaque core types (`Named`). Core unification treats two
named types as equal only when their names match; the surface checker owns
field presence, field types, order, and exhaustiveness. This keeps named
declarations distinct from the tensor unit `1`.

A record declaration's *representation* is the right-nested tensor of its
field types — `+i32 ⊗ +i32` for the declaration above — and a record with no
fields is the tensor unit `1`. A record literal and a record pattern must both
write every declared field exactly once, in declaration order, with the
declared type.

A record *value* is that tensor labelled by the declaration's name, exactly as
an `enum` variant is its payload labelled by the variant's name. One core form
covers both, which is why one surface form — `match` — takes either apart.

A `data` value carries every field at once, which is what `⊗` means: the
positive product of its field types, associated to the right. `data
Direction { left: i32, right: i32 }` describes `+i32 ⊗ +i32`, and the surface
tuple `(a, b)` is the same connective written anonymously.

A product is taken apart by `match`/`select`, which binds every component, or
by **projection** for a single one: `t.0`, `t.1`, … reads a tuple component,
and `s.field` reads a record field. Because the product is right-nested with
its last component stored bare, projection is resolved against the value's
type — the checker turns `.i` and `.field` into the component index — and then
walks the spine to it. A nested tuple and a flat one of the same shape are the
same value, so `(a, (b, c)).1` is `b`, not `(b, c)`: projection sees the flat
spine. `examples/projection.sl` uses both forms.

### Explicit connective types

Both multiplicative connectives are available as explicit *type* syntax, and
are always parenthesized:

```sl
fn sum_pair(p: (+i64 ⊗ +i64)) -> i64 { … }
command consume_pair | (k: (-i64 ⅋ -i64)) { … }
```

| Type syntax | Meaning |
|---|---|
| `(A ⊗ B)` | positive product; the anonymous form of a two-field `data` |
| `(A ⅋ B)` | negative product; the dual of `⊗`, a joint consumer of both sides |
| `(A -> B)` | function: `-A ⅋ B`. So a function is negative, `(A -> ⊥)` *is* `-A`, and `dual(A -> B)` is `A ⊗ -B` — an argument together with a continuation for the result, which is what a call stack is |
| `dual(A)` | the dual of `A`, applied — `dual(+i64)` *is* `-i64`, and `dual(dual(A))` is `A`. Only a declaration's name stays wrapped, since it is opaque to the core |
| `⊥` | bottom |

`⊗` and `data` are the same connective: a `data` declaration names a
product and its fields, while `(A ⊗ B)` writes one anonymously. Neither is
sugar for the other — a named declaration is opaque to core unification, while
an explicit tensor is structural.

### Negative multiplicative construction

The consumer of a product is built the same way, by `select`. A product has
exactly one shape, so it has exactly one arm, and that arm binds every
component — which is what makes it multiplicative rather than additive: the
halves arrive together, in one command, sharing its context.

```sl
data Reading { value: i64, unit: String }

// dual(Reading) is `-i64 ⅋ -String`: one consumer with both halves
fn show(out: -String) <- Reading {
    select Reading {
        Reading { value, unit } => (int_to_str(value) + unit) @ out,
    }
}
```

A bare product needs no declaration; its shape is written as the type:

```sl
fn total(out: -i64) <- (+i64 ⊗ +i64) {
    select (+i64 ⊗ +i64) {
        (left, right) => (left + right) @ out,
    }
}
```

Either is consumed by the cut that supplies the whole product:

```sl
Reading { value: 42, unit: "m" } @ show(out)
(2, 40) @ total(out)
```

### `form`: the negative multiplicative declared

`form` names that consumer, the way `menu` names the negative additive. Its
fields say what flows *in*, so `form Report { value: i64, label: String }`
denotes `-i64 ⅋ -String` — the dual of the record its fields describe.

```sl
data Report { value: i64, label: String }   // ⊗ every field, given
form Report { value: i64, label: String }   // ⅋ every field, wanted
```

The two negative declarations follow one rule: **the literal syntax builds
the demand, and the value is built by the keyword of its core family.** A
form is a consumer — μ̃ family — so `select` builds it; a menu is a μ[…]
value, so `mu` builds it. A menu's demand is one labelled request,
`.item(k)`; a form's is the whole record, `Report { … }`, whose type is the
form's dual.

```sl
fn printer(out: -i64) -> Report {
    select Report {
        Report { value, label } => { println(label); value @ out },
    }
}

Report { value: 42, label: "answer" } @ printer(k)
```

`form` needs nothing new in the core: a form value is the `co(μ̃[…])` that
`select` over a product already builds, and its demand is that product
labelled — so the two meet by the existing labelled rule. What the
declaration adds is a *name* for the consumer side, so a signature can say
`-> Report` instead of spelling out `dual(…)`, and the fields of that
consumer can be named.

A form is always fed whole: there is no `form.field`. From `-A ⅋ -B` no `-A`
can be extracted, though `A ⊗ B` yields its `A` — reading a field off a
record discards the others, and a form would instead have to *invent* them.
That is not a gap in the implementation but the shape of the connective, and
it is why `⅋` is not a record in any usable sense.

An atom is the degenerate product: one shape, one component. `select` covers
it too, and the arm's pattern is a plain binder that names the whole value:

```sl
fn show(out: -String) <- +i64 {
    select +i64 {
        n => int_to_str(n) @ out,
    }
}
```

That is the surface spelling of the core's value abstraction `μ̃x. c` — the
same binder `let` lowers to, written directly; `examples/mu_tilde.sl` writes
that one co-term every way the surface offers. So `select` builds the consumer
of *any* positive type, with no exceptions: one arm per variant for a sum, one
arm binding every component for a product, one arm binding the value for an
atom.

### Traits

Impls dispatch on either side of the mirror: a `menu` or a `form` carries
one exactly as a `data` or an `enum` does — `impl Describe for Config`
with the method demanding `self.retries` — including bounded impls for
generic menus.

A bounded impl — `impl<T: Display> Display for List<T>` — keys by the
declaration's name and covers every instantiation; its dictionary is
**constructed** at each use, the impl's global applied to one dictionary per
bound, read off the use's type arguments and built recursively:
`fmt` over a `List<List<i64>>` passes
`__dict_Display_List(__dict_Display_List(__dict_Display_i64))`. (v1 limit:
a *multi-method* trait's bounded impl cannot be constructed yet, and says
so.)

A `trait` names operations over an implicit `Self`; an `impl` gives them for a
type; a bound `<T: Show>` lets a generic use them. A method is a free function
overloaded on its first argument's type — `show(x)`, never `x.show()`:

```sl
trait Show { fn show(self: +Self) -> String; }
impl Show for i64  { fn show(self: +i64)  -> String { int_to_str(self) } }
impl Show for bool { fn show(self: +bool) -> String { if self { "t" } else { "f" } } }

fn labelled<T: Show>(x: +T) -> String { "= " + show(x) }
```

The checker makes dispatch total: coherence allows one `impl` per trait and
type, and a method call is accepted only when the type has an impl — a ground
type directly, a bounded type parameter through its bound, and an unbounded
one not at all.

Dispatch is then resolved entirely at compile time, with no runtime method
value. A call on a concrete type compiles to a direct call to its impl. A call
on a bound type parameter `<T: Show>` projects the method from a *dictionary* —
the trait's impls for `T`, passed to the bounded function as a hidden
argument; the function forwards it to any bounded call it makes, so the impl
is chosen once, by whoever supplied the concrete type. A single-method trait's
dictionary is just its impl.

A method may be a `command`, taking continuations like any other; the
dispatch is unchanged. Method names are unique across traits in v1, bounds are
on positive type parameters, and associated types, default methods, and
supertraits are not yet provided.

### Effects and handlers

An `effect` names operations a computation may perform; a `handle` answers
them. Performing an operation suspends the computation and passes control to
the nearest enclosing handler. An operation is a demand, so its clause binds
the carried continuation the copattern way — after a colon, under any name
(`resume` by convention) — or omits it, for a clause that never resumes:

```sl
effect Exn    { fn throw(message: +String) -> i64; }
effect Reader { fn config() -> i64; }

command main | (exit: -i32) {
    let safe = handle checked_div(10, 0) {
        throw(message) => 0 - 1,           // never resumes: an exception
        return(n) => n,
    };
    let scaled = handle x * config() {
        flip(): resume => resume(true) + resume(false),  // resumes twice
        return(n) => n,
    };
    …
}
```

An operation is a free function, the dynamic mirror of a trait method: a
trait is an operation table keyed by a *type* and resolved statically — the
dictionary travels with the value — while an effect is an operation table
keyed by the *stack* and resolved dynamically: a handler is installed by
`handle` (the effect it handles is inferred from its clause operations, not
written), and its clauses bind the captured continuation, which no trait
has. That mirror
is static-versus-dynamic provisioning; the *polarity* dual of effects is a
different axis — latency, below — and the two cross: `impl Trait for Menu`
is the static column's negative row, latent rows the dynamic column's.

A function declares the effects it may perform in an **effect row** on its
arrow — `fn scaled(x: +i64) -> i64 / {Reader}` — and a bare arrow is the
empty row, a pure function: the signature tells the whole truth, and every
declaration is checked locally against its own row. **Row polymorphism is
written the way the rest of the language writes generics, explicitly**: a
row variable is declared as a generic parameter and used with the `..`
"rest" spelling, and a parameter's arrow type carries the row calling it
may incur —

```sl
fn map<A, B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
```

A call instantiates the callee's row variables from the arguments standing
at the positions that mention them: `map(half, xs)` sets `E` to `half`'s
row, so the call incurs exactly what `half` performs. `{Exn, ..E}` extends
a variable — what the written part covers does not flow through it. A
rowless arrow in a parameter's type is a promise of purity, enforced at
the call site: passing `risky` where `(i64 -> i64)` is declared is an
error at the argument. A handler discharges the effects of the operations
it answers, and `main`'s row must be empty, so a well-typed program
performs no unhandled operation.

**Latent rows are the dual of effects.** A function's row fires at
application, because a function is a suspended producer: the work runs
before the value exists. Codata is the mirror — a menu answers per demand,
a form runs when fed — so its work runs *after* the value exists, on the
consumer's schedule, and its row belongs to the *type*:

```sl
menu Fallible / {Exn} { value: i64, doubled: i64 }
```

The arms of a `mu` over a rowed menu (and of a `select` over a rowed form)
are checked against the declaration's latent row and charged to no
function; every demand `f.value`, and every feed of a rowed form's record,
incurs the row — so the handler that discharges it is the one around the
*demand*, and one value can answer different demands under different
handlers. A consumer type carries latency the same way: `-> (-A / {..E})`
says the returned consumer performs `..E` when *fed*, not that the call
performs anything — a returned `fn`/`select` literal is checked against
that latent row, the cut it is eventually fed at incurs it, and a `handle`
around the mere construction discharges nothing, because nothing fired.
Latent rows on declarations are concrete in this version (a row variable
on a type is the rows-into-types upgrade), rowed declarations' item names
must be distinct, and a rowless menu or form keeps the conservative
account: its arms are charged to the declaration that wrote them.

The tracking follows names, conservatively where a function value loses
its name: a lambda's body is charged to the declaration that wrote it, a
higher-order global passed on as a value contributes its concrete row but
no further forwarding, and a function laundered through a `let` binding is
not tracked — though a `let` of a call whose result carries a latent row
keeps that row on the name.

An operation may take several parameters; since calls are curried, the
performing value collects them all before suspending. **Operations are
positive, and need no negative form.** An operation that consumes rather
than answers is already writable: `A → ⊥` *is* `-A`, so
`fn drop(x: +i64) -> ⊥;` declares a consumer, and both `drop(42)` and the
cut `42 @ drop` perform it. Routing to a chosen outcome needs nothing new
either, now that consumers are values — an operation takes them as
ordinary parameters, and the clause cuts into whichever it picks:

```sl
effect Judge { fn judge(n: +i64, ok: -String, bad: -String) -> ⊥; }
…
judge(n, ok, bad) => if n > 3 { "big" @ ok } else { "small" @ bad },
```

Demand-time effects are the latent rows above. Between the three, a
`<- T` operation form would add spelling, not power, so the grammar keeps
operations to `-> T`.

A clause may resume any number of times — the continuation is a first-class
value sliced from the one frame stack. Not resuming is an exception; resuming
once (with work after it, which composes) is a reader or state; resuming
twice is nondeterminism, the same captured continuation run with two answers.
Operation names are unique across effects.

Bounds and effect rows are independent of a function's polarity: a negative
function carries them in the same places — `fn emit<T: Show>(out: -String)
<- i64 / {Log}` — because a bound constrains a type parameter and a row
describes what the body performs, neither of which depends on whether the
function returns a value or a consumer.

A bound on a negative function is discharged by the **cut**, not by an
argument: in `fn emit<T: Display>(out: -String) <- T`, nothing the call
receives mentions `T`, and `42 @ emit(s)` is what fixes it. So dictionary
solving waits until a declaration's body is fully checked — by then every
cut has spoken — and the same deferral gives a trait a second method
shape:

```sl
trait Describe { fn describe(self: +Self) -> String; }   // receives Self
trait Deliver  { fn deliver(out: -String) <- Self; }     // consumes Self
```

A positive method takes `self: +Self` and dispatches on what it receives.
A negative method takes no `self` — a negative function's parameters are
all continuations — so its `Self` is the type it *consumes*, and dispatch
reads the value the cut sends: `42 @ deliver(s)` finds the `i64` impl,
`true @ deliver(s)` the `bool` one. Both shapes resolve statically, and a
bound forwards through either.

### Polymorphism

Two forms, one discipline. A declaration may take type parameters —
`fn id<T>(x: T) -> T` — which are rigid inside their own body and
instantiated afresh at every call. And a `let` generalizes, under the
**value restriction**: only when its right-hand side is a syntactic value —
a literal, a `fn`, a `select`, a constructor, record, tuple, or box of
values, a plain name. Every use of such a binding instantiates its variables
afresh:

```sl
let f = fn(x) { x };
println(f(1) + 1);           // a := +i64
println(str_len(f("s")));    // a := +String
```

Anything that computes stays monomorphic — `mu { k <= c }` above all, and every
application. A value ran nothing, so no two instantiations can disagree
about anything that happened; a computation may have captured its
continuation, and generalizing that is the classical unsoundness (the
Harper–Lillibridge counterexample is a `mu` returning a polymorphic
function; with continuations that resume, it would execute). When the
per-use behaviour is wanted, write it: `fn(u) { mu { k <= … } }` is a value,
generalizes, and visibly re-runs its capture at each use.
`examples/polymorphism.sl` shows all three: the generic declaration, the
generalized `let`, and the by-name idiom.

### Generic declarations

Every type declaration takes parameters, written as a function's are:

```sl
enum List<T> { Nil, Cons(T, List<T>) }
data Boxed<T> { inner: T }
menu Stream<T> { head: T, tail: Stream<T> }
```

A use applies the declaration — `List<i64>`, `Stream<String>` — and every
construction instantiates the parameters fresh: a variant, a record
literal, a bare `List::Nil`, or a `mu` over a generic menu, whose arms then
constrain the arguments. Patterns and `select` arms instantiate from the
scrutinee's arguments instead, so `Cons(n, rest)` over a `List<i64>` binds
`n: +i64` and `rest: List<i64>`. Recursion through the declaration's own
name gives inductive data — `List` — and, through a `menu`, coinductive
codata: `Stream<T>` is an infinite structure of which only the demanded
branches ever run.

Lists themselves are not built in: `List<T>` and its functions (`length`,
`map`, `append`, and the outcome-offering `command nth`) are prelude
declarations like any other. There is no list literal — a list is written
the way any enum value is.

### No shifts: a consumer is a value

The language once had the polarity shifts `↓`/`↑`, boxing a consumer as
data and marking the computation returning a value. They were removed:
they erased at lowering — a boxed consumer and the consumer were already
the same value at run time — and the declared negatives made them
redundant. A menu or form value always stored bare, so the box taxed only
the *structural* negatives; and both shift roles are one declaration away
when a name is wanted — `menu Lazy<T> { force: T }` is the computation
returning `T`, and a one-field `form` is a named, storable consumer.

So a consumer travels bare everywhere a value does: an enum payload
(`Refutes(-i64)`), a record field, a `fn` value parameter — passing a
continuation is an ordinary application, `handle(k)`. `dual` is an
involution on the nose: `-(-T)` *is* `T`, and double-negation elimination
is the identity function.

```sl
fn dne<T>(t: -(-T)) -> T { t }

dne(42)   // 42: -(-i64) and +i64 are one type
```

One orientation rule remains, and it is load-bearing: **the left of `@` is
the value side**. Without it, the involution would let any positive value
pass for a consumer of consumers — `dual(-i64) = +i64`, so `⟨k ∥ 42⟩`
would type — and the machine only runs cuts whose right side really
consumes. For the same reason `select` still consumes a positive type. A
continuation is a value everywhere except there, where it must be the one
doing the consuming.

### What may be left unwritten

A declaration is an interface, so its parameters carry types. Everything
inside one may leave a type out when something else already says it:

| written                                         | may be omitted when                                                                     |
|-------------------------------------------------|-----------------------------------------------------------------------------------------|
| a lambda's parameter and result: `fn(x) { … }` | always — the body is checked against how the value is used                             |
| a `mu`'s produced type: `mu { k <= … }`        | the arm hands `k` to a slot whose type is declared, or cuts a value against it          |
| a `select`'s type: `select { … }`              | an arm's pattern names it, or the enclosing negative `fn` already said what it consumes |

```sl
// `k` goes to a slot `read_file` declares, so it is `-String`, and this
// `let` binds a `+String`.
let source = mu { k <=
    read_file("input.json", k, complain)
};

// `Red` is a variant of exactly one enum, so the type is `Color`.
fn code(return: -i32) <- Color {
    select {
        Red => 0 @ return,
        Green => 1 @ return,
    }
}

// Nothing in the arm names a type, but `<- +i64` did.
fn twice(out: -i64) <- +i64 {
    select {
        n => (n * 2) @ out,
    }
}
```

What is left is what nothing else says. `select { n => println(n) }` bound to
a `let`, outside any negative `fn`, is rejected: no arm names a type and no
declaration supplied one, so it is written.

There is deliberately **no** way to build such a consumer out of two
independent consumers, and no way to feed one half at a time. Both are the
same thing — halves that progress independently — and both need either a send
that returns or concurrency. A cut does not return, and the language has no
concurrency, so a `⅋` is supplied whole.

## 9. Entry point and exit

A program is a command, so its entry point is a `command`. It takes no values
and exactly one continuation — the exit status:

```sl
command main | (exit: -i32) {
    println("Hello, Slant!");
    0 @ exit
}
```

The runtime supplies that continuation; the cut that reaches it is what ends
the program, and the integer it carries is the process exit status.

`exit` is a parameter, and it is the *only* door out. There is no global exit
consumer: ending the program is a right a helper is handed — as a continuation
parameter, or captured into a consumer built where `exit` is in scope — never
one it takes for itself. (An earlier design had a top-level `EXIT`; it let any
function end the program behind `main`'s back, and it is gone.)

```sl
command main | (exit: -i32) {
    let complain = select { message => { println(message); 1 @ exit } };
    read_file("input.txt", select { text => { print(text); 0 @ exit } }, complain)
}
```

Because a `command` body must be `⊥`, **every terminating path of a program
leaves through `exit`** — a `main` that falls off the end is rejected by the
type checker, not by a runtime convention.

There is no final-result value. A program's output is exactly what it prints;
its status is what it sends to `exit`. A `fn main`, a `main` with value
parameters, a `main` whose row is not one exit status, and a missing `main`
are all rejected.

The evaluator runs on a dedicated stack, so how deeply a continuation-passing
program nests is bounded by memory rather than by the host's default stack.

## Standard library

The library has two layers. The **prelude** is ordinary Slant source
(`crates/slc-driver/src/prelude.sl`): the driver appends it to the program
before parsing — the program's text comes first, so its spans and line
numbers are untouched — and everything in it goes through the same checking
and lowering as user code. A program's own declaration shadows a prelude
name (per namespace: values and type declarations separately). Shadowing a
prelude *type* strands the prelude functions that mention it — a program
declaring its own `Stream` loses `take` — so shadow a type only to replace
its whole family. Anything
expressible in the language belongs here rather than in the runtime.

The prelude defines the logical units `Unit`, `Bottom`, `Empty`, and `Top`;
`min`, `max`, `abs`; the `List<T>` enum with `length`,
`append`, `map`, and the outcome-offering `command nth`; **`Option<T>`**
and **`Result<T, E>`** with `unwrap_or` — either/or outcomes are additive,
so they are enums whose consumers are `select`s (a `form` would be the
wrong connective: it wants every field at once); the negative side's
**`Stream<T>`** — the coinductive mirror of `List`, with `repeat`,
`count_from`, `map_stream`, and `take` bridging back to data, since an
infinite structure cannot print whole and `fmt(take(s, n))` is the honest
form — and **`Lazy<T>`**, the one-item menu that is a by-name thunk; the
**`Display` trait** — `fn fmt(self: +Self) -> String`, user-facing
formatting as in Rust, with impls for `i64`, `String`, `bool`, and
`List<T>` (elementwise, `[1, 2, 3]`), plus `to_string<T: Display>` — and
three **consumer combinators**: `then(f, k)` — the composition of a function with a
continuation, `-A` from `A → B` and `-B` — `traced(label, k)`, a tap that
logs what passes through and forwards it, and `defaulting(fallback, k)`, a
`-String` failure consumer that discards the message and sends `fallback`
onward, made for the multi-outcome builtins below.

The combinators share one shape, and it is forced: a negative `fn`'s
parameters form its continuation row, and values do not ride in a row — so
a combinator that needs a value input is a *positive* fn returning the
consumer, built as a λ whose body is a command, since `A → ⊥` *is* `-A`.
The `<- A` declaration form remains the natural spelling for pure consumer
transformers, whose inputs are all continuations.

**Builtins** are what the language cannot express — I/O, arithmetic on
machine integers, string internals — and they follow the same rule the
language does: **a builtin whose outcome
is a single value is an ordinary function; a builtin whose outcome is not —
it can fail, or find nothing — takes continuations and denotes a command.**
The value arguments come first, then one continuation per outcome, and exactly
one of them is activated.

| Builtin      | Values                                           | Outcomes                                            |
|--------------|--------------------------------------------------|-----------------------------------------------------|
| `parse_int`  | `text: +String`                                  | `ok: -i64`, `invalid: -String`, `overflow: -String` |
| `read_file`  | `path: +String`                                  | `ok: -String`, `failed: -String`                    |
| `open_file`  | `path: +String`                                  | `opened: -File`, `failed: -String`                  |
| `read_line`  | `handle: +File`                                  | `line: -String`, `end: -unit`                       |
| `write_file` | `path: +String`, `contents: +String`             | `ok: -unit`, `failed: -String`                      |
| `char_at`    | `text: +String`, `index: +i64`                   | `ok: -char`, `out_of_range: -String`                |
| `find_char`  | `text: +String`, `from: +i64`, `character: +i64` | `found: -i64`, `absent: -String`                    |

Every failure continuation receives a `+String` describing what happened, so
it composes with an error consumer a program already has.

```sl
read_file(
    "input.json",
    select +String { source => parse_json(source, report) },
    complain,
)
```

A consumer per outcome is what `select` builds, so an outcome's handler can be
written where it is passed rather than declared elsewhere.

Everything else is a function: `println`, `print`, and `format`; arithmetic and
comparison; `str_len`, `str_concat`, `int_to_str`, `str_eq`, `substring`;
`is_digit`, `is_ws`, `skip_ws`, `skip_digits`; `file_exists`; `close_file`,
which spends a handle so a later read through it fails; and the
`list_` constructors and totals (`list_new`, `list_len`, `list_push`).

A handle is a value of its own base type, `+File`, produced only by
`open_file` — so nothing else closes a file or reads a line. Closing on every
terminating path is not checked; today an
unclosed handle merely leaks until the program ends, and a read after
`close_file` is a runtime error.

Until a resource check watches it, the program can make the leak impossible by
construction: compose the close onto the only door out, by shadowing `exit`
where the handle comes into scope.

```sl
let handle = mu { k <= open_file(path, k, complain) };
let exit = select +i32 { status => { close_file(handle); status @ exit } };
```

The arm's `exit` is the outer one; everything after the shadow sees only the
composed door, so every later `@ exit` — unhappy paths included — closes the
file on its way through. `examples/file_io.sl` is written this way.

Two failures stay fatal rather than becoming outcomes: an out-of-range index
`s[i]` and a division by zero. They are reached through operator syntax, which
has nowhere to put a continuation, and — as in Rust, where `v[i]` panics while
`v.get(i)` does not — they report a bug in the program rather than a case it
was meant to handle. The checked forms are the `command`-shaped builtins above.

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

`A → B` is `-A ⅋ B`, which is why this works: `A → ⊥` is `-A ⅋ ⊥`, and `⊥` is
the unit of `⅋`. The same identity gives the dual: `dual(A → B)` is `A ⊗ -B`,
an argument together with a continuation for the result — a *call stack*. So
`f(v)` and the cut of `f` against the pair `(v, k)` are the same interaction,
and a consumer of a function is an ordinary value of that product type.

A cut is well typed exactly when its two sides are dual. Which side is
written negatively is not itself the question: `v @ k` sends `v` to something
that consumes it, and for a function that something is a call stack.

An integer literal takes the integer type its port requires — `0 @ exit`
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
| `exhaustiveness` | a `match` or `select` does not cover its alternatives exactly once                   |
| `lowering`       | an otherwise accepted surface construct cannot be translated to the core calculus    |

The compiler applies these phases in order:

1. `parse`
2. `type`
3. `polarity`
4. `exhaustiveness`
5. `lowering`

A phase stops before later phases once it reports a diagnostic. Consequently,
`parse` diagnostics take precedence over all checker diagnostics; `type`
diagnostics take precedence over polarity and exhaustiveness; and
so on. Within one phase, diagnostics are source-ordered. Checker diagnostics
include `line:column` positions and source excerpts.

Runtime failures are not compiler diagnostics. They are reported after
evaluation begins and do not participate in this precedence order.

## 10. Modules

A `mod` is a named scope of declarations, `::` reaches into it, and `use`
brings one name into scope:

```sl
mod geometry {
    enum Shape { Circle(i64), Rect(i64, i64) }

    fn area(s: Shape) -> i64 { … }     // its own names are bare here
}

use geometry::area;

command main | (exit: -i32) {
    println(area(geometry::Shape::Circle(5)));
    0 @ exit
}
```

Modules exist only to resolution, which runs right after parsing: every
declaration inside `mod m` is renamed `m::name`, every reference is rewritten
to the qualified name it resolves to, and the `mod` and `use` declarations
disappear. The checker, the lowering, and the runtime never see them — they
work on flat names, which always contained `::`, because an enum variant is a
path already.

A name resolves in scope order: a local binding shadows everything and is
left alone; then the enclosing module's `use` aliases; then its own
declarations; then each ancestor's, out to the root. A path resolves by its
first segment and keeps the rest, so `inner::deep()` works from a sibling and
`geometry::Shape::Circle` from anywhere. A name nothing claims is left bare
for later passes — that is how builtins stay global. Two `use` declarations
bringing in the same name are an error.

Modules are single-file, everything is public, and `main` must be declared at
the root — a `main` inside a module is `m::main`, which the entry point does
not accept.

### Variant imports

`use` has three forms:

```sl
use module::name;        // one member, aliased into this module
use Colour::*;           // every variant of an enum, bare
use Colour::{Red, Blue}; // the listed variants, bare
```

A bare variant name resolves in this order: an explicit import pins it — an
import that collides with another import, or names a variant its enum does
not have, is an error at the `use` — otherwise the automatic rule applies:
unqualified while exactly one enum declares the name. A bare name that
*several* enums declare and nothing imports is an **error**, not a binder:
a pattern that silently caught everything is the failure mode this rule
exists to kill. Imports are **scoped to their source unit**: the prelude
pins its own bare names with `use List::*;`, and that import reaches no
program code — just as a program's `use Mine::*;` never changes what the
prelude means, and the two never collide.

## 11. Core calculus

### Grammar

```text
Term      t ::= x                     variable
              | λx. t                 value abstraction
              | μα. c                 capture of the ambient continuation
              | t ⊗ t                 tensor pair
              | L(t)                  labelled additive injection (enum value)
              | μ[M; .d₁(α). c₁ | … ] menu (negative additive value)
              | μ[M]                  empty menu, retaining its named owner
              | co(e)                 a co-term reified as a value

CoTerm    e ::= α                     co-variable
              | t · e                 application: argument, then tail
              | μ̃x. c                 value abstraction
              | prj:i                  projection of the i-th component
              | μ̃[M; L₁(x…). c₁ | … ] labelled consumer (enum, data)
              | μ̃[M]                  empty labelled consumer, retaining its owner
              | μ̃(x₁, …, xₙ). c       product consumer
              | .d(e)                 request (destructor)

Command   c ::= ⟨ t ∥ e ⟩             cut

Type      A ::= +B | -B               positive / negative atom
              | A ⊗ A | A ⅋ A         multiplicatives
              | 1 | ⊥                 their units
              | A + A | A & A         additives
              | A → A                 function
              | dual(A) | Named | ?v  dual, declaration name, inference variable
```

`co(e)` reifies a co-term as a value: a consumer on the value side,
so that a consumer can sit where a value is expected. The surface never
writes it directly — `select` denotes a consumer and lowers straight to it,
and a continuation passed as an argument arrives the same way. Its
elimination is application: `⟨ co(e′) ∥ v · e ⟩ → ⟨ v ∥ e′ ⟩` sends the
argument to the underlying co-term. The reification is invisible at
lowering because the value is
already in this form; the type is what they change.

A declared continuation parameter and `μα. c` both bind a continuation
variable, and they are not interchangeable. A declared parameter is an
ordinary `λ` binder — the caller supplies the continuation, since a
continuation is a value like any other, and that is a `command`'s row.
`μα. c` captures the *ambient* continuation instead, and that is the `mu`
expression.

### Execution

The evaluator is an abstract machine in the shape the calculus suggests: a
state is what is being evaluated together with an explicit stack of frames —
the continuation, held as data rather than as host stack. A cut pushes the
co-term side as a frame; `μ` captures the stack into a value; a captured
continuation is activated by reinstating its stack, which is why it outlives
its `mu` and can be used more than once. `select` branches stay unevaluated
until activation chooses one, and a fuel bound turns divergence into an
error.

The machine does not walk the named core. The whole program is compiled,
once, into a single flat instruction stream — a `Chunk`, one vector of nodes
— and every sub-expression is a node index, the machine's instruction
pointer. In it, every lexical binder is resolved to a de Bruijn index, so a
variable reference is a count into a positional environment rather than a walk
comparing names. What a compiler cannot resolve lexically — a global, a
literal, or a `match` arm's pattern variables, which the pattern engine
injects at run time — stays a name, found in a by-name overlay and then the
globals table. Keeping pattern injections in their own overlay is what lets
the indices be stable: an injected binding never shifts the positional chain,
yet, being part of the environment, it is still captured by a closure that
escapes the arm.

The continuation and all three environment layers are persistent `Rc` conses
with their most recent entry at the head, so capturing the continuation
(`mu`, or a handler's `resume`) or cloning the environment (which the machine
does on nearly every step) bumps refcounts rather than copying — O(1)
regardless of depth, and a push never disturbs a handle captured earlier.

### Printed form

The grammar above is also the core's printed form: the compiler prints terms,
co-terms, commands, and types in exactly this syntax, and reads them back
unchanged. Printing is therefore a faithful view of the IR rather than an
approximation of it, and a printed declaration can be compared, stored, or
re-parsed without loss.

### Reduction

```text
⟨ μα. c ∥ e ⟩                → c[e/α]              μ
⟨ v ∥ μ̃x. c ⟩                → c[v/x]              μ̃ — the binder
⟨ λx. t ∥ v · e ⟩            → ⟨ v ∥ μ̃x. ⟨ t ∥ e ⟩ ⟩  → — application
⟨ co(e′) ∥ v · e ⟩           → ⟨ v ∥ e′ ⟩           apply the reified consumer
⟨ (t₀ ⊗ … ) ∥ prj:i ⟩        → tᵢ                  projection
⟨ L(v₁ ⊗ …) ∥ μ̃[M; … L(x…). c …] ⟩ → c[vᵢ/xᵢ]       labelled
⟨ μ[… .d(α). c …] ∥ .d(e) ⟩  → c[e/α]              copattern
⟨ co(.d(e)) ∥ μ̃[M; … .d(x). c …] ⟩ → c[co(e)/x]     co-labelled
⟨ v₁ ⊗ v₂ ∥ μ̃(x, y). c ⟩     → c[v₁/x, v₂/y]      product
```

The labelled rule is what makes `select` lazy: the label of the value selects
one branch, and the branches that were not selected are discarded unreduced.
The copattern rule is its mirror: the request selects one branch of the menu
and binds the continuation it carries. The co-labelled rule is what lets
`match` take a continuation apart — a reified request is a labelled
positive value, so the arm binds the request's own continuation as a value.
An `enum` has a branch per variant and a `data` exactly one, so the same
rule covers the additive and the labelled multiplicative; the product rule is
its unlabelled counterpart.

### Lowering table

Every accepted surface construct lowers as follows. `⟦e⟧` is the lowering of
`e`, and `f(a)` abbreviates the application `μ__call. ⟨ ⟦f⟧ ∥ ⟦a⟧ · __call ⟩`,
nested left to right for several arguments.

| Construct | Surface | Core |
|---|---|---|
| `expr.literal` | `42`, `"s"`, `'c'`, `true` | a constant variable (`$int_42`, `$str_"s"`, …) |
| `expr.ident` | `x` | `x` |
| `expr.enum` | `Color::Red`, `Shape::Circle(r)` | `Color::Red(unit)`, `Shape::Circle(⟦r⟧)` — several payload values pack into one tensor |
| `expr.call` | `f(a, b)` | `f(a)(b)` (curried application encoding) |
| `expr.lambda` | `fn(x: +A) -> B { e }` | `λx. ⟦e⟧` |
| `expr.pair` | `(a, b)`, `()` | `⟦a⟧ ⊗ ⟦b⟧`, right-nested; `()` is `unit` |
| `expr.let` | `let x = v; e` | `μlet. ⟨ ⟦v⟧ ∥ μ̃x. ⟨ ⟦e⟧ ∥ let ⟩ ⟩` — a binder is `μ̃`, the value abstraction |
| `expr.block` | `{ e₁; e₂ }` | `μ__seqᵢ. ⟨ ⟦e₁⟧ ∥ μ̃__discarded. ⟨ ⟦e₂⟧ ∥ __retᵢ ⟩ ⟩` |
| `expr.if` | `if c { t } else { e }` | `__if_dispatch(⟦c⟧, λ_. ⟦t⟧, λ_. ⟦e⟧)` — branches are thunks, so only the chosen one runs |
| `expr.binop` | `a + b` | `add(⟦a⟧)(⟦b⟧)`; `&&` and `\|\|` expand to `expr.if` first, keeping them short-circuiting |
| `expr.unop` | `-a`, `!a` | `neg(⟦a⟧)`, `eq(⟦a⟧)(false)` |
| `expr.index` | `a[i]` | `__index(⟦a⟧)(⟦i⟧)` |
| `expr.slice` | `a[i..j]` | `substring(⟦a⟧)(⟦i⟧)(⟦j⟧)` |
| `expr.cut` | `v @ k` | `μ__cut. ⟨ ⟦v⟧ ∥ k ⟩` for a named consumer, and `μ__cut. ⟨ ⟦k⟧ ∥ ⟦v⟧ · __tail ⟩` for a computed one — evaluate the consumer, then apply it, exactly as an application does. The μ binder is never referenced — a command has no result — and is renamed if the consumer is called `__cut` |
| `expr.mu` | `mu A { k <= e }` | `μk. ⟨ ⟦e⟧ ∥ k ⟩` — the captured continuation, not a declared parameter; the type in front is what the expression produces |
| `expr.match` | `match s { p => e, … }` | a match the core can express — every arm a shape (variant, record, tuple, request, or one whole-value binder), components binders or nested products, no guards, no duplicates — is a genuine cut: `μ__match. ⟨ ⟦s⟧ ∥ μ̃[T; L(x…). ⟨⟦e⟧ ∥ __match⟩ \| … ] ⟩` (`μ̃(x…)`/`μ̃x` for a product/atom). Anything order-sensitive — guards, literals, or-patterns, a default among labelled arms — falls back to `__match_dispatch(⟦s⟧, arm₁, …)`, each arm `__match_arm(descriptor ⊗ (guard ⊗ λ__match_arg. ⟦e⟧))` |
| `expr.data` | `S { f: v, g: w }` | `S(⟦v⟧ ⊗ ⟦w⟧)` — the declaration's name labelling the right-nested tensor of its fields, the same shape a variant has |
| `expr.select` | `select T { p => c, … }` | `co(μ̃[T; L(x…). ⟦c⟧ … ])` for a labelled type — one branch per shape, the pattern's binders naming that shape's components — and `co(μ̃[T])` when it has no shapes; `co(μ̃(x…). ⟦c⟧)` for a product, and `co(μ̃x. ⟦c⟧)` for an atom, whose one binder takes the whole value |
| `expr.comatch` | `mu T { item: k <= c, … }` | `μ[T; .T::item(k). ⟦c⟧ | …]` — the copattern form of `mu`: a menu value, one branch per demand. Nested copatterns group by their outer destructor: the branch binds `__k`, and its body cuts the inner menu against it |
| `expr.request` | `.item(k)` | `co(.M::item(k))` for a named continuation; any other expression is bound first, then named. A demand `cfg.item` is `μ__ask. ⟨ ⟦cfg⟧ ∥ .M::item(__ask) ⟩` |
| `decl.menu` | `menu M { item: A, … }` | no term of its own: `mu M { … }` builds the `μ[…]`, and its items name the `.M::item(e)` requests |
| `decl.form` | `form F { field: A, … }` | no term of its own: `select F` builds `co(μ̃[F; F(x…). ⟦c⟧])`, and `F { … }` builds the demand `F(⟦v⟧ ⊗ …)` it consumes |
| `expr.consumer_argument` | `f(k)` — a consumer as an argument | `⟦k⟧` — a consumer is a value; nothing to coerce |
| `decl.fn.positive` | `fn f(x: +A) -> B { e }` | `λx. ⟦e⟧` |
| `decl.fn.negative` | `fn f(k: -A) <- B { e }` | `λk. ⟦e⟧` |
| `decl.mu` | `command f(x: +A) \| (k: -B) { e }` | `λx. λk. ⟦e⟧` |
| `decl.const` | `const C: +A = v;` | `⟦v⟧` |
| `decl.enum` | `enum E { V }` | one global per variant: `E::V = E::V(unit)` |
| `decl.data` | `data S { … }` | no term; the declaration is a type |

Binders are nested in declaration order, so a call supplies arguments in the
order the parameters are written; value and continuation parameters alike
become λ binders.

### Surface-to-core coverage

| Core construct | Surface representation |
|---|---|
| `x`, `λx. t` | identifiers, functions, lambdas, and declared continuation parameters (`fn … <- …`, a `command`’s row) |
| `μα. c` | local `mu` expression, `@` against a named consumer, and the lowering of `let`, blocks, and applications |
| `t ⊗ t` | tuple literals, `data` literals, `(A ⊗ B)` values |
| `L(t)` | `enum` values and `data` values — a labelled product |
| `μ[M; .d(α). c \| …]` | `mu` over a `menu` — the copattern form |
| `.d(e)` | a demand `cfg.item`, and the consumer inside a request literal `.item(k)` |
| `co(e)` | `select`, and every consumer in value position — a reified co-term, and a `form` value |
| `α` | the consumer named on the right of a cut, `v @ k` |
| `v · e` | application, and nothing else — `f(a)`, and a cut whose consumer is computed rather than named (`v @ f(a)`), which is the same act: applying the consumer the expression evaluates to |
| `μ̃x. c` | every binder: `let`, a discarded block expression, an `if`'s condition; written directly as `select +A { x => c }` |
| `μ̃[T; …]`, `μ̃[T]` | `select` over an `enum` or a `data`, including an empty enum |
| `μ̃(x…)` | `select` over a bare product |
| `prj:i` | `base.i` (tuple) and `base.field` (a record), the field resolved to its index from the base type |

### Classical control

The core is classical, so the classical laws are ordinary programs. Negation
is a consumer — `¬A` is `-A`, since `A → ⊥` and `-A` are one type — and both
laws are written with `mu`, which hands out the continuation of the expression
it stands in:

```sl
// ¬¬A → A: give the refuter this call's continuation.
fn dne(refuter: +i64) -> i64 {
    mu { k <= k @ refuter }
}

// A ⊕ ¬A: answer with the refutation, which is the continuation in disguise.
fn lem() -> Choice {
    mu { k <=
        Choice::Refutes(select +i64 { a => Choice::Holds(a) @ k }) @ k
    }
}
```

`examples/classical.sl` runs both. The types above go through the shifts of
§8 — `-(-i64)` *is* `+i64`: `dne` is the identity, and `dne(42)` is `42`.

A captured continuation is a value with no expiry: the evaluator is an
abstract machine whose continuation is an explicit frame stack, and `mu`
captures by reifying it. Activating `k` *reinstates* that stack — after the
`mu` has answered, from however deep, as many times as it is reached — so
taking `lem()`'s offer re-enters the very `match` that already received
`Refutes`, which this time holds.

## 12. Error continuations

Fallible operations receive their result continuations directly. For example,
a parse operation receives both a success continuation and an error
continuation:

```sl
let parsed = select +String { value => { println("parsed: " + value); 0 @ exit } };
let failed = select +String { message => { println("error: " + message); 1 @ exit } };
parse_json(source, parsed, failed)
```

No result wrapper is needed, and nothing carries a success value alongside an
error value: the continuation that is activated *is* the outcome.

**A row of continuations is already the outcome type.** The consumer of
`A ⊕ B` is a consumer of `A` together with a consumer of `B`, so declaring an
`enum` of outcomes and sending it to a single continuation adds a wrapper
without adding information — and it costs something, because the row can say
what a single continuation cannot: which outcomes each operation actually has.
In `examples/json_parser.sl` every parser takes `failed`, but only the
top-level one takes `parsed`, so no inner parser can report success by
mistake. Keep an `enum` for data that a program *holds*; outcomes that a
program *reaches* are a row.

## 13. Migration summary

- `k(v)` (activating a continuation) → `v @ k`
- `EXIT(0)` → `0 @ EXIT` → gone: end the program through a continuation
  parameter, the way `main` does with `exit`
- `select T { c => p }` → `select T { p => c }` — the shape comes first, as
  in a `match`
- `select T { V => k(v) }` → `select T { V => v @ k }`
- `t @ k` previously lowered to a μ binder that shadowed `k`, so it sent the
  value nowhere; it is now the cut it always claimed to be
- `mu name(...) | (...)` → `command name(...) | (...)` — a declaration is a
  `command`; `mu` is the expression that captures the current continuation
- `spawn` → removed; no replacement exists
- `mu(x, to k)` → `mu(x) | (k)`
- `agent.to(k, h)` and `agent.consume(k, h)` → removed; no partial-agent replacement exists
- `+fn` → `fn ... -> ...`
- `-fn` → `fn ... <- ...`
- bare `fn` → rejected; write either `->` or `<-`
- old variant-style `choose T { Variant }` → removed
- `choose Struct` → removed while its design is deferred
