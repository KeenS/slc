Part of the [language design](../../DESIGN.md).

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
tensor, so every variant carries exactly one payload.

`of` decomposes an enum by choosing the corresponding arm, and a variant
pattern binds exactly the payload its variant declares:

```sl
of shape {
    Point => 0,
    Circle(r) => (<(3, r) | mul | x => (x, r) | mul),
    Rect(w, h) => (<(w, h) | mul),
}
```

### Negative additive construction

`mu` builds the consumer of any positive type by giving, for each shape
that type can take, a command. For an `enum` that is one arm per variant — the
negative additive:

```sl
func k(return: i32) <- Color {
    mu Color {
        Red => <0 | return>,
        Green => <1 | return>,
        Blue => <2 | return>,
    }
}
```

The consumer transformer receives the consumer continuation `return`; activating
the constructed continuation with an enum value dispatches to the matching arm,
which activates that arm's consumer. Arms must cover exactly one enum variant
each, must be exhaustive, and must not repeat variants.

An arm may bind the payload of its variant and pass it to the consumer, which
is how a value reaches the continuation the variant selects:

```sl
enum Reading { Measured(i64), Missing }

func report(value: -i64, absent: -i64) <- Reading {
    mu Reading {
        Measured(measurement) => <measurement | value>,
        Missing => <-1 | absent>,
    }
}
```

`Measured(measurement)` binds the payload of `Measured` for that arm only. A
variant that carries a payload must bind it; a variant that carries none must
not. A component that is itself a product — a tuple or a record — may be
taken apart in place, `Rect((w, h))` or `At(Point { x, y }, d)`: a product
has one shape, so deeper destructuring keeps the one-arm-per-shape law. A
*sum* inside a component cannot be split across arms, and there are no
literal arms: a branch table answers each shape exactly once, unordered,
while literals make the arm list ordered, first-match — that is `of`,
inside the arm.

An arm writes `=>`, exactly as an `of` arm does, because the arrow marks
which side of the mirror the scrutinee is on: **data flows forward into an
arm (`=>`); a demand reaches back into it (`<=`)**. A `mu` matches data,
so its arms are `pattern => proc`; a `mu` answers demands, so its arms are
`copattern <= proc`; and an `of` writes whichever its scrutinee calls
for — `p => e` over a value, `.item(out) <= e` over a continuation. The
command an arm runs must be a cut `v | k` whose consumer is a visible
negative binding. The arm lowers to it, so a `mu` expression is a genuine negative
additive consumer — one branch per variant — and not an opaque builtin. Activation
chooses exactly one branch: the branches of the arms that were not selected
are never evaluated, neither when the consumer is constructed nor when it is
activated.

### `menu`: the negative additive declared

`menu` declares the negative additive type itself — the mirror of `enum`. An
enum value is one tagged variant the producer chose; a menu value answers one
item the consumer demands:

```sl
enum Config { Retries(-i64), Name(-String) }   // ⊕ — the value picks
menu Config { retries: i64,  name: String }    // &  — the demand picks
```

The nullary case is the additive unit ⊤, written `(&)`: the menu with no
items, which answers no demand, and whose one value is `(&)` itself. Its core
form is `μ[(&)]`, an empty branch table retaining that owner. The four units
are the nullary forms of their connectives — `(,)`, `(|)`, `(&)` and `(;)` —
and structural: no declaration names them (*Connective spellings*, §8).

The two declarations above are each other's dual: `dual(i64 & String)` is
`-i64 ⊕ -String`, a sum of requests each carrying the continuation that wants
the answer — the shape the enum writes with explicit boxes. `menu` makes that
type native, so the boxes and the encoding disappear.

Each keyword owns one core family, so the branch tables split by what
arrives at them:

- **`mu` answers data** — it builds the μ̃ family: the consumer of an
  atom, a product, an enum, or the record a `form` consumes. One arm per
  shape the data can take.
- A `mu` arm is `item: out <= c`: it binds the demand's continuation — its
  *return address*, carried the way a variant's payload is — and answers it
  with a command. The continuation can receive a direct answer with
  `item <= <value | item>`, or route control through nested copatterns and
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
  (`examples/laziness/stream.sl`).
- **`of` — a branch table applied to a named scrutinee, on either side.**
  Over an enum value it takes data apart; over a continuation of a menu type
  (`k: -Config`) it takes the *request* apart: `.item(out) => e` binds the
  request's own continuation, and the arms are ordinary expressions —
  typically other requests. An arm has no guard: a test on what a pattern
  bound is an `of` inside the arm. A choice on a `Bool` is an `of` on
  it, `of c { True => …, False => … }`.

```sl
func config() -> Config {
    mu Config {
        retries: out <= <3 | out>,
        name: out <= <"slant" | out>,
    }
}

func reroute(k: -Config) -> -Config {
    of k {
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
- `v | request` **cuts** a menu against a request directly; `mu` names where
  the answer goes.

A menu type is negative, but a menu is a *value*: it may be returned
(`-> Config`), passed as a value parameter, and sit at the left of a flow — the
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

A record declaration's *representation* is the tensor of its
field types — `+i32 ⊗ +i32` for the declaration above — and a record with no
fields is the tensor unit `1`. A record literal and a record pattern must both
write every declared field exactly once, in declaration order, with the
declared type.

A record *value* is that tensor labelled by the declaration's name, exactly as
an `enum` variant is its payload labelled by the variant's name. One core form
covers both, which is why one surface form — `of` — takes either apart.

A `data` value carries every field at once, which is what `⊗` means: the
positive product of its field types, one component each. `data
Direction { left: i32, right: i32 }` describes `+i32 ⊗ +i32`, and the surface
tuple `(a, b)` is the same connective written anonymously.

A product is taken apart by `of`/`mu`, which binds every component, or
by **projection** for a single one: `t.0`, `t.1`, … reads a tuple component,
and `s.field` reads a record field. Projection is resolved against the
value's type — the checker turns `.i` and `.field` into the component index —
and reads that component. Nesting is significant: `(a, (b, c))` has two
components, so its `.1` is `(b, c)`, while `(a, b, c)` has three. A record's
field is read by binding the record's fields under its label, so a record of
one field is read the same way. `examples/basics/projection.sl` uses both forms.

Building data does not demand its negative components. A record field and a
tuple component follow the same by-name rule, as do a named variant's payload
and `::i(e)`. Projection and pattern binding retrieve the stored computation
without running it; applying the retrieved function runs it afresh under the
handler around that application. Its row must fit the declared component
type, even when a handler surrounds construction. Positive components still
compute during construction, in written order. `let+` on a constructor
evaluates that constructor, not recursively its negative components.
`examples/laziness/by_name_components.sl` shows both polarities and repeated demand.

Projecting from a delayed bundle first runs the computation that produces
the bundle, under the handlers around the projection. Repeating the
projection repeats that computation; neither the bundle nor its aliases
cache the result. Projection then retrieves the chosen item without forcing
it if it is itself delayed. Thus constructing a bundle may require `Build`
while applying its retrieved callback separately requires `Use`. An effect
handler that resumes construction several times completes the pending
projection in each resumption. `examples/laziness/delayed_bundle.sl` demonstrates the
construction and item-demand boundaries.

### Explicit connective types

Every connective is available as explicit *type* syntax, always
parenthesized; a paren joins any number of components with one connective,
and nesting is significant — `(A, (B, C))` is not `(A, B, C)`:

```sl
func sum_pair(p: (i64, i64)) -> i64 { … }
proc consume_pair | (k: (-i64 ; -i64)) { … }
```

| Type syntax | Meaning |
|---|---|
| `(A, B)` | a **tuple**: positive product; the anonymous form of a two-field `data` |
| `(A \| B)` | a **choice**: positive sum; the anonymous form of a two-variant `enum` |
| `(A & B)` | a **bundle**: negative sum; the anonymous form of a two-item `menu` |
| `(A ; B)` | a **joint**: negative product; the dual of `,`, a joint consumer of both sides |
| `(A -> B)` | function: `(dual(A) ; B)`. So a function is negative, `(A -> (;))` *is* `-A`, and `dual(A -> B)` is `(A, dual(B))` — an argument together with a continuation for the result, which is what a call stack is |
| `dual(A)` | the dual of `A`, applied — `dual(+i64)` *is* `-i64`, and `dual(dual(A))` is `A`. Opaque named types and effect-annotated types retain an explicit wrapper: receiving an effectful answer must not activate or force it |
| `(,)`, `(\|)`, `(&)`, `(;)` | the units of `,`, `\|`, `&` and `;`: the empty tuple, choice, bundle and joint |

`,` and `data` are the same connective: a `data` declaration names a
product and its fields, while `(A, B)` writes one anonymously. Neither is
sugar for the other — a named declaration is opaque to core unification, while
an explicit tensor is structural.

### Connective spellings

The surface is ASCII: `⊗`, `⅋` and `⊥` are the notation of the core's terms,
never of a program or of its diagnostics, and this document's prose about a
surface type spells a joint with `;` too. Every connective is written three
ways — declared by name, anonymously, and nullary, as a paren holding only
its separator. Each anonymous type has a name of its own:

| named | anonymous | its name | anonymous type | value | unit type | unit value |
|---|---|---|---|---|---|---|
| `data` | tuple | a tuple | `(T1, T2)` | `(v1, v2)` | `(,)` | `(,)` |
| `enum` | choice | a choice | `(T1 \| T2)` | `::0(v)`, `::1(v)` | `(\|)` | — |
| `menu` | bundle | a bundle | `(T1 & T2)` | `(v1 & v2)` | `(&)` | `(&)` |
| `form` | joint | a joint | `(T1 ; T2)` | `(k1 ; k2)` | `(;)` | — |

A **tuple** gives every component; a **choice** gives one, at its
position; a **bundle** offers every item and answers the one demanded;
a **joint** wants every component, one continuation each. The names follow
the declarations they write anonymously, not their connectives' logical
names, so a program's vocabulary is the one its code already uses.

The multiplicatives are `,` and `;`, the additives `&` and `|`: each dual
pair is a pair of punctuation marks.

- **Choices** name their position, as `Enum::Variant(v)` does with the name
  left out: `::0(v)`, `::1(v)`, and the pattern `::0(x)`. Positions count
  from 0, as tuple projection `t.0` does, and a choice is built by its
  position alone, so its type need not be known to build it. Its payload
  meets the component at that position of the choice type its context gives
  — a return type, an annotation, a parameter or a cut — once the
  declaration is checked. `(T1 | (T2 | T3))` has two positions, the second
  itself a choice, so a value of it is `::1(::0(v))`. A `mu` over a
  choice answers each position exactly once; an `of` covers every one, or
  has an arm that matches anything. The consumer of a choice `(A | B)` is the
  bundle `(-A & -B)`, so a bundle of exits consumes a choice as it is: the
  position picks the exit. And a choice flows into a function that consumes
  it: `<::0(7) | describe | println`, with `describe(out: String) <- (i64 |
  String)`, reads `describe` the way round that takes a choice, and the chain
  carries on with the `String` it hands on.
- **Joints** are built from one continuation per component: `(k1 ; k2)` is
  a value of `(T1 ; T2)`. Fed a product `(a, b)`, delivery starts on the
  left, with `a` sent to `k1`. A genuine consumer does not return, so this
  transfer prevents delivery to `k2`; constructing a joint does not promise
  that all its components run. Its latent row includes the component rows.
  It is the consumer `mu (A, B) { (a, b) => … }`
  builds, so a joint and a `form` value keep one runtime shape. It has no
  pattern form, for the reason a form has none. A joint value is one
  consumer, a closure over a single command, and does not hold the
  continuations it was built from; one built by `mu` never had separate
  ones. A pattern `(k1 ; k2)` would have to invent them, as `form.field`
  would. What can be matched is what a joint is fed: `mu (A, B)` takes
  the product apart.
- **`A -> B`** stays, as the spelling of `(dual(A) ; B)`.
- **The units are structural.** `(;)` is the type of a command; `(|)` has no
  value, is consumed by `mu (|) {}`, and an `of` on one needs no arm;
  `(&)` is ⊤'s unique value. No unit has a name besides its spelling, and
  the core has all four: 1, 0, ⊤ and ⊥.

  `unit` is only a surface alias for `(,)`; it is not a fifth atomic type.

### Negative multiplicative construction

The consumer of a product is built the same way, by `mu`. A product has
exactly one shape, so it has exactly one arm, and that arm binds every
component — which is what makes it multiplicative rather than additive: the
halves arrive together, in one command, sharing its context.

```sl
data Reading { value: i64, unit: String }

// dual(Reading) is `(-i64 ; -String)`: one consumer with both halves
func show(out: -String) <- Reading {
    mu Reading {
        Reading { value, unit } => <(<value | int_to_str, unit) | add | out>,
    }
}
```

A bare product needs no declaration; its shape is written as the type:

```sl
func total(out: -i64) <- (+i64, +i64) {
    mu (+i64, +i64) {
        (left, right) => <(left, right) | add | out>,
    }
}
```

Either is consumed by the cut that supplies the whole product:

```sl
<Reading { value: 42, unit: "m" } | show | out>
<(2, 40) | total | out>
```

### `form`: the negative multiplicative declared

`form` names that consumer, the way `menu` names the negative additive. Its
fields say what flows *in*, so `form Report { value: i64, label: String }`
denotes `(-i64 ; -String)` — the dual of the record its fields describe.

```sl
data Report { value: i64, label: String }   // ,  every field, given
form Report { value: i64, label: String }   // ;  every field, wanted
```

The two negative declarations follow one rule: **the literal syntax builds
the demand, and the value is built by the keyword of its core family.** A
form is a consumer — μ̃ family — so `mu` builds it; a menu is a μ[…]
value, so `mu` builds it. A menu's demand is one labelled request,
`.item(k)`; a form's is the whole record, `Report { … }`, whose type is the
form's dual.

```sl
func printer(out: -i64) -> Report {
    mu Report {
        Report { value, label } => { <label | println; <value | out> },
    }
}

<Report { value: 42, label: "answer" } | (<k | printer)>
```

`form` needs nothing new in the core: a form value is the `co(μ̃[…])` that
`mu` over a product already builds, and its demand is that product
labelled — so the two meet by the existing labelled rule. What the
declaration adds is a *name* for the consumer side, so a signature can say
`-> Report` instead of spelling out `dual(…)`, and the fields of that
consumer can be named.

A form is always fed whole: there is no `form.field`. From `(-A ; -B)` no `-A`
can be extracted, though `(A, B)` yields its `A` — reading a field off a
record discards the others, and a form would instead have to *invent* them.
That is not a gap in the implementation but the shape of the connective, and
it is why a joint is not a record in any usable sense.

An atom is the degenerate product: one shape, one component. `mu` covers
it too, and the arm's pattern is a plain binder that names the whole value:

```sl
func show(out: -String) <- +i64 {
    mu +i64 {
        n => <n | int_to_str | out>,
    }
}
```

That is the surface spelling of the core's value abstraction `μ̃x. c` — the
same binder `let` lowers to, written directly; `examples/duality/mu_tilde.sl` writes
that one co-term every way the surface offers. So `mu` builds the consumer
of *any* positive type, with no exceptions: one arm per variant for a sum, one
arm binding every component for a product, one arm binding the value for an
atom.

Traits are specified in [Traits](traits.md). Effects and handlers are
specified in [Effects](effects.md).

### Polymorphism

Two forms, one discipline. A declaration may take type parameters —
`func id<+T>(x: T) -> T` — which are rigid inside their own body and
instantiated afresh at every call. And a `let` generalizes, under the
**value restriction**: only when its right-hand side is a syntactic value —
a literal, a `func`, a `mu`, a constructor, record, tuple, or box of
values, a plain name. Every use of such a binding instantiates its variables
afresh:

```sl
let nothing = Maybe::Nothing;
<(nothing, 1) | or_else | println;      // T := +i64
<(nothing, "s") | or_else | println;    // T := +String
```

A lambda is a value too, but its parameter must have a known polarity by
the end of the declaration, so `let f = fn(x) { x }`, which nothing pins
down, is refused and asks for an annotation: a polymorphic function is a
declaration, `func id<+T>(x: T) -> T`.

Anything that computes stays monomorphic — `mu { k <= c }` above all, and every
application. A value ran nothing, so no two instantiations can disagree
about anything that happened; a computation may have captured its
continuation, and generalizing that is the classical unsoundness (the
Harper–Lillibridge counterexample is a `mu` returning a polymorphic
function; with continuations that resume, it would execute). When the
per-use behaviour is wanted, write it: `func { mu { k <= … } }` is a
value, and visibly re-runs its capture at each use.
`examples/basics/polymorphism.sl` shows all three: the generic declaration, the
generalized `let`, and the by-name idiom.

### Generic declarations

Every type declaration takes parameters, written as a function's are, each
with its polarity:

```sl
enum List<+T> { Nil, Cons(T, List<T>) }
data Boxed<+T> { inner: T }
menu Stream<+T> { head: T, tail: Stream<T> }
```

A use applies the declaration — `List<i64>`, `Stream<String>` — and every
construction instantiates the parameters fresh: a variant, a record
literal, a bare `List::Nil`, or a `mu` over a generic menu, whose arms then
constrain the arguments. Patterns and `mu` arms instantiate from the
scrutinee's arguments instead, so `Cons(n, rest)` over a `List<i64>` binds
`n: +i64` and `rest: List<i64>`. Recursion through the declaration's own
name gives inductive data — `List` — and, through a `menu`, coinductive
codata: `Stream<T>` is an infinite structure of which only the demanded
branches ever run.

Lists themselves are not built in: `List<T>` and its functions (`length`,
`map`, `append`, and the outcome-offering `proc nth`) are prelude
declarations like any other. There is no list literal — a list is written
the way any enum value is.

### No shifts: a consumer is a value

Here "shifts" means polarity-shifting type wrappers, not composable control
capture. The library operation `control::shift` in
[§6](control.md#6-mu-capturing-the-current-continuation) is unrelated.

The language once had the polarity shifts `↓`/`↑`, boxing a consumer as
data and marking the computation returning a value. They were removed:
they erased at lowering — a boxed consumer and the consumer were already
the same value at run time — and the declared negatives made them
redundant. A menu or form value always stored bare, so the box taxed only
the *structural* negatives; and both shift roles are one declaration away
when a name is wanted — `menu Lazy<*T, E> / {..E} { force: T }` is the
explicit computation returning `T`, and a one-field `form` is a named,
storable consumer.

`Lazy<T, E>` is an ordinary one-item menu, accepting either polarity of
`T`; `.force` performs `E` and returns `T` without activating that result.
`Lazy<T>` has an empty demand row. `(-> T / E)` instead annotates an
implicit computation and accepts only negative `T`. They are not literal
duals: `dual(Lazy<T, E>)` is the menu's request type, carrying a continuation
for its answer, not a delayed computation. For negative `T`,
`lazy::of_delayed` and `lazy::to_delayed` convert between the two interfaces
without running the computation at conversion time. Both preserve
call-by-name: repeated demands repeat construction. Neither caches results.
`examples/laziness/delayed_and_lazy.sl` contrasts the interfaces and their handlers.

So a consumer travels bare everywhere a value does: an enum payload
(`Refutes(-i64)`), a record field, a `func` value parameter — passing a
continuation is an ordinary application, `<k | do`. `dual` is an
involution on the nose: `-(-T)` *is* `T`, and double-negation elimination
is the identity function.

```sl
func dne<+T>(t: -(-T)) -> T { t }

<42 | dne   // 42: -(-i64) and +i64 are one type
```

One orientation rule remains, and it is load-bearing: **the left of `|` is
the value side**. Without it, the involution would let any positive value
pass for a consumer of consumers — `dual(-i64) = +i64`, so `⟨k ∥ 42⟩`
would type — and the machine only runs cuts whose right side really
consumes. For the same reason `mu` still consumes a positive type. A
continuation is a value everywhere except there, where it must be the one
doing the consuming.

### What may be left unwritten

A declaration is an interface, so its parameters carry types. Everything
inside one may leave a type out when something else already says it:

| written                                         | may be omitted when                                                                     |
|-------------------------------------------------|-----------------------------------------------------------------------------------------|
| a lambda's parameter and result: `fn(x) { … }` | how the value is used fixes the parameter's polarity by the end of the declaration    |
| a `mu`'s produced type: `mu { k <= … }`        | the arm hands `k` to a slot whose type is declared, or cuts a value against it          |
| a `mu`'s type: `mu { … }`              | an arm's pattern names it, or the enclosing consumer transformer already said what it consumes |
| a type's sign: `x: +i64`, `k: -i64`             | it agrees with the position — see below                                                 |

**A sign is omitted where the position implies it.** The table in
[§4](polarity.md#polarity-by-position) has a
diagonal: a value parameter is positive, a continuation row is negative, and
the type after `<-` is positive. On the diagonal the sign says nothing the
position had not already said, so it is left out — `proc nth<+T>(xs:
List<T>, i: i64) | (found: T & missing: String)` is the same declaration as
the fully signed one. Off the diagonal the sign *is* the information, and is
written: `note: -String` receives a consumer as data, `-> -T` returns one,
`Refutes(-i64)` carries one in a variant, and `-(-T)` is double negation. The
implication reaches into an `&` written out in a row — a menu of exits is
still a menu of exits — but not into a joint `(A ; B)`, an arrow, or a `dual`, each of
which states its own polarity.

Writing the implied sign stays legal: an explicit sign is a constraint, and
on the diagonal it is one the position already meets. `examples/duality/polarity.sl`
writes all four cells out, because the four cells are its subject.

```sl
// `k` goes to a slot `fs::read` declares, so it is `-String`, and this
// `let` binds a `+String`.
let source = mu { k <=
    <"input.json" | fs::read | (k & complain)>
};

// `Red` is a variant of exactly one enum, so the type is `Color`.
func code(return: i32) <- Color {
    mu {
        Red => <0 | return>,
        Green => <1 | return>,
    }
}

// Nothing in the arm names a type, but `<- i64` did.
func twice(out: i64) <- i64 {
    mu {
        n => <(n, 2) | mul | out>,
    }
}
```

What is left is what nothing else says. `mu { n => <n | k> }` bound to
a `let`, outside any consumer transformer, is rejected: no arm names a type and no
declaration supplied one, so it is written.

`(k1 ; k2)` builds a joint from separate consumers, but does not make them
progress independently. There is no way to feed one half at a time: a joint
is supplied whole, and has no destructuring pattern. Sequencing returning
sinks uses ordinary functions of type `A -> (,)`, not consumers `-A`.
A `mu` arm must be a command, never a unit-valued returning sink; a cut
does not return to the statement following it.
