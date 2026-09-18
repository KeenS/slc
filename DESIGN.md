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
`examples/duality/two_styles.sl` writes one program both ways.

Code in this document is of two kinds. A fragment marks what it leaves out
with `…`. A complete program declares `main` and leaves nothing out, and the
test suite compiles every one (`crates/slc-driver/tests/design_programs.rs`).

## 1. Design goals

1. **Rust-like surface** — familiar `fn`, `command`, `let`, `match`, braces, type
   annotations, and paths.
2. **λ̄μμ̃ core** — terms, co-terms, and cuts are the underlying semantic
   categories.
3. **Polarized types** — positive types denote values/proofs; negative types
   denote continuations/refutations.
4. **Explicit control** — a continuation is activated by a cut, `v | k`,
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

## 3. Flow: application, composition, and the cut

Everything moves left to right through one operator. `a | b` is **flow**,
and what a step means follows from polarity — no other reading is
available, so none has to be chosen:

Every step is a function applied to what flows in. **`<` opens the chain**
with a value — the head is what flows in — and **`>` closes it**: the stage
before it consumes, so the chain delivers rather than returns.

| form | what it is |
|---|---|
| `<v \| f` | a value through a function — an application, and a value |
| `<v \| f \| k>` | **a cut** — a command, `⊥` |
| `f \| g` | function composition — a function |
| `f \| k>` | composition into a consumer — a consumer |

**A stage can be adapted to read either way round.** `(A ; B)` is
`dual(A) -> B`; turning it gives `(B ; A)`, or `dual(B) -> A`.
A returning function and its consumer-transformer counterpart can therefore
serve the same pipeline, through an elaborated adapter rather than
unrestricted type equality:

```sl
fn area(s: Shape) -> i64                  // (-Shape ; +i64)
fn area_of(out: i64) <- Shape             // (+i64 ; -Shape) — adapted orientation
```

Either stands as a stage, and what flows in picks the reading; the forward
reading wins when both fit. A stage read the second way
takes *the rest of the chain* as its continuation, which is why the two
styles are written the same:

```sl
<shape | area    | label    | out>
<shape | area_of | label_of | out>
```

`examples/duality/two_styles.sl` is that program, twice.

The same adapter is available wherever a value meets a declared type. A
consumer transformer stored in a menu item declared `(i64 -> String)`, a returning function
passed where `(-String -> -i64)` is declared, or either kept in a record
field, a variant, a `let`, or returned, is accepted at the other spelling.
A value of a joint type is a closure facing one way, so the checker records a swap
there and lowering turns the closure around:

```
f : left ⅋ right   ↦   λk. μx. ⟨ f x ∥ k ⟩        a positive left
                   ↦   λk. co(μ̃x. ⟨ f x ∥ k ⟩)    a negative left
```

— capturing with `μ` where the binder is a genuine continuation, building
the consumer with `μ̃` where it is a genuine value, and cutting toward `k`
or from it by the polarity of `right`. The forward reading is always tried
first, so nothing that fits as written changes meaning. A tuple or an
alternative written out is turned component by component, each component
one value meeting one declared type, and a stage's result that meets the
next stage at the other spelling is turned around between the two steps.
**Adapters lift through structure, not just literals.** A stored tuple,
alternative, record, or enum can be adapted componentwise. Functions adapt
their inputs in the opposite direction and their results in the forward
direction. Menus adapt the answer to the item actually requested; forms
and other named consumers adapt the demand they receive. The compiler
derives these adapters from declarations, including regular recursive
declarations, rather than treating different representations as equal.

```sl
data Box<-F> { value: F }

fn deliver(out: String) <- i64 {
    select i64 { number => <number | int_to_str | out> }
}

command main | (exit: i32) / {IO} {
    let original = Box { value: deliver };
    let adapted: Box<(i64 -> String)> = original;
    <7 | adapted.value | println;
    <0 | exit>
}
```

Parameter polarity still applies: `Box<-F>` accepts these negative stages;
the standard `List<+T>` does not. Polarity is not variance. Dual occurrences
use the reverse adapter's dual; they do not simply map inputs forward.
Tuple order is unchanged except when adapting an explicitly dual parameter
requires the dual of a `;` reversal. There is no general tuple permutation
or commutative type equality.

**Turning preserves forcing as well as activation.** Adapting
`Delayed<T, E>` to `Delayed<U, E>` stores an adapter for the result. Each
demand first forces the original computation under that demand's handlers,
then adapts the result, without activating it. An eager `let+` therefore
performs construction but not activation; another demand of the original
delayed value repeats construction. Neither row may be erased, merged into
the other, or moved across a handler boundary. Adapting a structure does
not force delayed payloads or request unselected menu items.

Lifting requires a finite, bounded adapter derivation from available
declarations. Recursive specialization whose type arguments keep growing
is refused. Opaque constructors, including `Handler`, still require matching
arguments; capability rows are checked invariantly rather than mapped.
Exact matching remains the first choice. These are elaborated adapters,
not an unrestricted equality law under every type constructor.

`examples/duality/structural_adapters.sl` demonstrates stored and recursive values
and the separate construction and activation phases. Implementation details
and the validation obligations are in `docs/design-notes/structural-adapters.md`.

**`<` is never left out.** A chain without it begins with a function,
whatever its head is, and composes: `f | g` is a function, and `f | k>` a
consumer. So a value must be marked to flow in — `<"hi" | println` applies,
and `"hi" | println` is refused, since `"hi"` is not a function. A chain
says what it is at both ends: `<` makes it an application, `>` a delivery,
and the two together a cut. And `<` needs a stage to send its value into:
`<1` alone is refused, since the value on its own needs no mark. A function sent on as a value is marked the
same way as any other value:

```sl
f | k>          // compose f into k: a consumer
<f | k>         // send f itself to k: a cut
```

A flat chain and its nested applications share one elaboration:
`<v | f | g` and `<(<v | f) | g` have the same demand boundaries.
Composition `<v | (f | g)` follows those same boundaries. Each stage receives
its argument by the polarity rule in §4: positive computations run before
the stage, negative computations wait for demand. The rule includes a
computed final consumer: a positive input runs before that consumer is
constructed. Intermediate negative results can be discarded or demanded
repeatedly. These are regrouping laws, not permission to reorder effects,
insert eager bindings, or move expressions across handlers. **A consumer
stands only at the right end**, since nothing flows out of one.

A chain ends where the expression holding it does: at the `;` or `}` of a
block, and at the `,` or `)` that closes a component. So a chain stands in a
tuple, a bundle or a data literal's field without parentheses of its own —
`<(<p | read_text, "!") | add` — while a tuple written *after* `|` is one
stage, and `<x | (f, g)` does not end at its `,`.

Two operations look alike in most languages and are different here.

**Application** is flow: `<a | f` supplies an argument to a function and
gets a result. An ordinary declared function with arguments uses flow:
`f(a)` is refused, with the pipeline spelled out. Several arguments are the product they
always were, written as one: `<(a, b) | f`. So a call and a chain are not
two things to learn, and reading either goes left to right:

```sl
<21 | double | label | println          // apply, four times over
<(xs, 2) | index_or_zero               // several arguments, one product
```

**A stage supplies the whole group.** A declaration binds each parameter
group as one argument, so what flows into a stage is all of its values or
it is refused: `<1 | add` of a two-parameter `add` is not a function waiting
for the second, and says so. (A callee's type nests by `;`'s associativity
and presents its first parameter alone; the checker does not read it that
way.) Builtins are no exception: the runtime happens to accumulate a
builtin's arguments one at a time, but a stage still supplies the whole
group, checked against the builtin's signature as a declaration's is — so
`<(1, "b") | add` is refused before it runs.

Nullary returning functions use `f()` or `<(,) | f`; the name `f` alone
is a function value of type `((,) -> T / {E})`, not an invocation. Naming
it performs nothing. A parameterless consumer transformer instead denotes
the consumer it declares; naming it does not activate that consumer.
Primitive and trait-method calls retain their parenthesized compatibility
forms, with the same argument demand rules as flow. A variant constructor
`Cons(h, t)` *builds*, and keeps its parentheses.

**A `command` is a stage too.** It takes two groups — values, then the
menu of exits — and the chain hands it both: what flows in is the value
group, and the closing stage is the row. So a command reads like every
other call, and ends where control leaves it:

```sl
command nth<+T>(xs: List<T>, i: i64) | (found: T & missing: String)

<(xs, 2) | nth | (found & missing)>
```

The row travels whole, so a command that takes one may hand it on
unopened — `command forward(…) | (row: (-T & -String)) { <(xs, 2) | nth | row> }`.
A consumer transformer is *not* this case: it answers a consumer rather than
`⊥`, so it composes on, and its exits are the rest of the chain
(`<shape | area_of | label_of | out>`).

**A stage can name what it is given.** A stage that takes more than what
flows in would otherwise need the chain so far packed into a tuple with the
rest, one level of nesting per such stage. Instead, a stage after `|` may
begin with a binder, one for each side of the chain:

- `x => e` names the value flowing in and passes on `e`, built from it. It
  is the function `fn(x) { e }`, standing as a stage.
- `k <= e` names the consumer the rest of the chain builds, and gives the
  stage before it `e`, built from it. So the chain must close on a consumer,
  and something must follow the binder.

```sl
<1 | stream::count_from | seq::of_stream
   | s => (odd, s) | seq::filter
   | s => (s, 4)   | seq::take            // [1, 3, 5, 7]

// `halve` offers `(ok: i64 & odd: String)`: each step supplies its failure
// exit, and the chain carries on with the success.
<12 | halve | ok <= (ok & odd) | halve | ok <= (ok & odd) | out>
```

A binder builds nothing itself, so every connective is written in its own
syntax — `x => ::1(x)` for a choice, `k <= (k ; other)` for a joint — and the
name says what is abstracted, where a placeholder would leave open which
parenthesis it belongs to. Its body is one stage, ending at the next `|`,
and only a stage after `|` can be one: the head of a chain and a `match` arm
keep their meaning.

**A cut** `<v | k>` sends the value `v` to the consumer `k`. It is the surface
spelling of the core's `⟨ v ∥ k ⟩`, and it is a *command*, not an expression
that happens to return: control does not come back, so nothing after it in a
block runs, and its type is `⊥`.

```sl
command route(x: i32) | (k: i32) {
    <x | k>
}
```

There are no infix operators, so nothing competes with `|` for precedence:
arithmetic and comparison are the prelude's trait methods — `add`, `sub`,
`mul`, `div`, `rem`, `neg`, `eq`, `ne`, `lt`, `gt`, `le`, `ge` — that a group
flows into, so `<(a, b) | add | k>` sends the sum. A `-` touching a number is
part of it, `-1`, and a `String`'s character at a position is
`<(s, i) | index`, a slice of it `<(s, i, j) | substring`.
The consumer may be any expression that produces one — a name, or a
consumer transformer applied to its row:

```sl
<Color::Blue | code | answer>     // `code` is a stage; `answer` closes
```

Because a cut has type `⊥`, an arm that ends in one constrains nothing: in
`match c { True => <(pos, 1) | add, False => <message | err> }` the `match` has the type of
the arm that returns, and arms that both return must agree.

Calling a continuation is rejected. `k(v)` reports that `k` is a consumer and
not a function, because a reader — and the compiler — should not have to know
what `k` is bound to in order to tell an application from a command.

## 4. Function orientation and evaluation polarity

A **returning function** uses `->` and produces a value:

```sl
fn plus(x: +i32, y: +i32) -> i32 {
    <(x, y) | add
}
```

A **consumer transformer** consumes continuations and produces a continuation. It is
written with the reverse arrow:

```sl
enum Status { Ok(i64), Failed(i64) }

fn report(success: -i64, failure: -i64) <- Status {
    select Status {
        Ok(code) => <code | success>,
        Failed(code) => <code | failure>,
    }
}
```

The arrows identify the direction of the cut:

- `fn(value_params) -> Output` consumes values and produces a value.
- `fn(continuation_params) <- ContinuationType` consumes continuations and
  produces a continuation.

There is one `fn` declaration form. The old `+fn` and `-fn` prefixes do not
exist. Both orientations have negative function types; "returning" does not
mean positive polarity or call-by-value evaluation. Argument and result
types determine evaluation independently of the declaration's orientation.

A consumer-transformer declaration is an ordinary negative abstraction. It does not
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
  returning `fn` may take one, because it returns rather than ending in a cut,
  so it promises nothing about consuming it; a `command` splits its parameters by
  polarity, so a consumer there belongs in the continuation group instead.
- A **positive type in continuation position** is the type after `<-`. A
  continuation is named by the type it consumes, so `fn config() <- Request`
  writes a positive type and produces its consumer.

Consuming codata reverses a cut's usual sides: a provider is negative, so what
consumes it is its dual — the positive request. In
`<Request::Retries(answer) | provider>` the provider is the consumer and the
request is the value.

`examples/duality/polarity.sl` writes all four; `examples/errors/polarity_error.sl` writes
the two a `command` rejects.

A type the checker has not solved yet still has a polarity once it meets a
generic parameter that states one: meeting `<+T>` makes it positive, meeting
`<-T>` negative, and standing under a `dual` flips it. So a lambda parameter
used only where `<+T>` is declared needs no annotation, and a type that meets
both a `<+T>` and a `<-T>` is refused — no type is both positive and
negative — before anything solves it.

### Continuation rows

The continuation parameters of a consumer transformer, and the second parameter
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

A row is supplied as one menu — the chain's closing stage — so a caller
writes exactly one bundle, and a row of the wrong width is a type error
at that stage rather than a miscount of arguments.

A bundle item is a by-name position, as a tuple component and an argument
are: a computation of negative type there is delayed, and runs only when the
consumer chooses it. So an item that ends in a cut — a block
`{ <0 | exit> }`, or a bare cut — is an exit of type `(;)` that jumps when it
is taken, `<(,) | then>`, and not while the bundle is built:

```sl
<c | pick | ({ <"then" | println; <0 | exit> } & { <1 | exit> })>
```

A value ran nothing and is passed as it is. Primitive arguments obey the
same polarity rule as other arguments, including direct calls and aliases.
A scalar primitive receives computed positive values; an outcome primitive
forces and activates only the callback it selects. A primitive inspecting
an opaque negative value, such as internal `__display`, does not demand it
just to print its representation. There is no blanket eager-argument
exception for primitive names.

An exit is taken by naming it where a command stands — an arm, a block's
statement or its end — and passed on by naming it anywhere else:

```sl
command pick<E>(c: Bool) | (then: Delayed<(;), ..E> & otherwise: Delayed<(;), ..E>) / {..E} {
    match c { True => then, _ => otherwise }       // runs the exit
}

command forward<E>(c: Bool) | (then: Delayed<(;), ..E> & otherwise: Delayed<(;), ..E>) / {..E} {
    <c | pick | (then & otherwise)>               // passes them on
}
```

What *is* enforced is that control is **total**: a `command` body must be `⊥`
— it reaches a continuation on every path — so a body that falls off the end
(a bare value) or dangles (a `match` with an arm that yields a value) is
rejected by the type checker, not by any linearity pass. A call
supplies each row position a continuation of exactly the declared type.

An argument whose type the checker cannot determine — an unannotated `let`
binding, for instance — is not rejected; a row mismatch is reported only for
an argument whose type is known.

### Generic function parameters

A declaration may declare type parameters, and each states its polarity on
the declaration: `<+T>` ranges over positive types, `<-T>` over negative
ones, and `<*T>` accepts either polarity without assuming which. A type
variable carries no polarity of its own, so the mark is
required — on type declarations, functions, commands and impls alike — and
it goes on the declaration because `-T` in a type already means `dual(T)`.
A row variable, used as `..E`, ranges over effects rather than types and
takes no mark: `fn map<+A, +B, E>`.

The mark is held against every use. A call, a function named as a value, and
a construction — a variant, a record, a `mu` over a generic menu — give each
parameter a type once the declaration's unification has finished, and a
positive parameter given a function, a consumer or a menu is refused, as is
a negative one given data. Inside a generic body a parameter carries its own
mark, so `fn f<-U>(x: U) -> U { <x | id }` is refused when `id` declares
`<+T>`. A type written in a declaration's signature is held to the same rule:
with `enum List<+T>`, `List<-i64>` and `List<(i64 -> i64)>` are refused, and
a list of consumers is a declaration of its own.

An unrestricted parameter can forward or store its argument, but cannot
assume it is positive or negative. `fn id<*T>(value: T) -> T { value }`
accepts both data and functions. Passing its `T` to a `<+U>`-only function
is refused. A computation whose result has unrestricted polarity needs
an explicit evaluation choice if inference cannot settle that polarity.

The mark on a generic parameter fixes the polarity of its instantiation.
With `<+T>`, `T` is positive; a bare `T` in a continuation row denotes its
dual consumer, not a negative instantiation of `T`:

```sl
fn id<+T>(value: T) -> T { value }
fn consume<+T>(ok: T) <- T { ok }
```

An explicit sign also states the position's polarity: `+T` is rejected in
a continuation row, while `-T` explicitly requests the dual of a positive
`T`. A returning function may receive an explicitly negative value parameter;
its declaration orientation does not make all its parameters positive.
Generic function declarations are type-erased at
lowering: their ordinary parameters lower to λ binders and their continuation
parameters lower to λ binders as well — a continuation is a value like any other.

### A binder is a pattern

`let p = e`, and a parameter `p: T`, take a pattern; a bare name is the
trivial one. A binder stands for **every** value of its type — there is no
other arm to fall to — so the pattern must be irrefutable: tuples, bundles,
records, single-variant enums, and `_` all qualify, and a many-variant enum
is refused with a pointer at `match`.

```sl
let (a, b) = pair;
let Point { x, y } = origin;
fn skew((a, b): (i64, i64), c: i64) -> i64 { <(a, c) | mul | x => (x, b) | sub }
```

This is what the unary calling convention already stood on. A declaration
binds one argument per group, and a header *is* a pattern with typed leaves:
the value group a tuple pattern on the one argument, the continuation group a
bundle pattern on the one menu of exits. Writing a pattern at a leaf only
says out loud what the group was doing already, and nesting goes as deep as
it likes.

A **continuation parameter is a name**. Control leaves through it, and a
pattern has nowhere to leave through — the group as a whole is the bundle
pattern, and its leaves are the names of the exits.

`let … else` is out of scope: a binder that may fail is a `match`.
`examples/basics/patterns.sl` writes all of it.

### When a `let` computes

A binding says when its value is computed. `let+` computes it where it is
written, whatever its type — the way to perform a computation's effects under
the handler in scope. `let-` does not compute it there: the name holds the
computation, which runs afresh each time its result is demanded — applied,
cut into, projected as a bundle, or asked for a menu item — and is passed on
unrun when it is bound again, stored, or supplied as an argument.

Eager binding also forces an existing implicit delay. With
`let- pending = make(); let+ ready = pending;`, construction runs at the
second binding, under the handlers surrounding it. `ready` holds the
resulting value; `pending` is unchanged and recomputes on its next demand.
This does not invoke a resulting function, demand a resulting menu item,
or recursively force fields. It is explicit evaluation, not memoization.

```sl
let- shout = { <"made" | println; fn(s: String) { <s | println } };
<"a" | shout;       // made, a
<"b" | shout;       // made, b
```

What `let-` holds is negative — a function, a consumer, a menu — and a
positive type is refused: a value delayed would bring back the `↑` that §8
removed, so `Lazy<T, E>` stays the explicit menu spelling for a delayed
positive result. It also accepts negative results. `let-` binds a name,
since a pattern takes apart a value and a delayed computation is not one
until it runs. In the core a delayed computation is `λ$delay. t`, a thunk of
the unit, and the runtime runs it where it is demanded.

A binding is one of the **by-name positions**. The others are what flows
into a chain — the argument it applies, `<e | f` — an argument written in
parentheses, a tuple component, a record field, a variant or positional-choice
payload, and a bundle item. In each, a computation of negative type is delayed
and a positive one computed when the enclosing constructor is evaluated.
Nothing is cached: a delayed value demanded twice runs twice, effects
included, as a continuation resumed twice does. A name of type `(;)`
standing as a command runs the exit it holds (see "Continuation rows").

A plain `let` follows the polarity of its type. A negative computation is
delayed, as `let-` would; a positive one is computed where it is written, as
`let+` would; and a value — a literal, a name, a `fn`, a `select`, a
constructor of values — ran nothing, so it is bound as it is. Which one a
binding is may be known only once the declaration's unification has
finished, so it is settled then, and a computation whose polarity is still
unknown is refused, asking for an annotation, `let+` or `let-`.

The same inference applies to every by-name position, not just bindings.
Preliminary inference supplies unresolved polarities for placing effects;
the final type check validates those choices before lowering marks delays.
An unresolved or changed choice is rejected rather than checking effects as
immediate while lowering the computation as delayed. An unrestricted generic
`*T` computation needs an explicit evaluation choice, such as `let+`, because
its result may have either polarity.

Effects follow the same rule. A delayed computation performs nothing where it
is written: what it performs happens at each use, under the handlers around
that use, so a `let-` written inside a `handle` and used after the `handle`
has returned answers to the handlers outside it. `let+` is how a computation
performs under the handler where it is written. What a delayed computation
performs rides on its type as a row (§8, "Effects and handlers"), so it
follows the computation wherever it goes — bound again, stored in a tuple,
record or alternative, handed to a callee — and is performed wherever it
finally runs.
Where its type meets a slot that allows less, a parameter declared as a pure
arrow for instance, it is refused there.

**Forcing is not activation.** `Delayed<T, E>` annotates an implicitly
delayed computation of negative result type `T`. Its forcing row is `E`;
the result keeps its own activation row. For example:

```sl
data Saved { callback: Delayed<(i64 -> i64 / {Use}), {Build}> }
```

Constructing the function performs `Build`; applying the result performs
`Use`. Ordinary application of the delayed callback does both. An eager
binding performs only `Build` and binds `(i64 -> i64 / {Use})`. A plain
function type promises no effectful forcing: replacing the field above
with `(i64 -> i64 / {Build, Use})` is not equivalent. The same distinction
survives parameters, returns, aliases, fields and menu answers. Empty
forcing rows normalize away, so `Delayed<T, {}>` and `Delayed<T>` are `T`
as effect promises, without changing call-by-name evaluation.

Put the eager binding *inside* the construction handler:

```sl
let+ ready = handle { let+ value = pending; value } {
    build(): resume => <10 | resume
};
```

Merely handling the expression `pending` does not force it, and a `let+`
outside that handler cannot move forcing back inside it.

```sl
let g = handle { let- f = make(); f } { throw(m) => fn(n: i64) { 0 } };
<5 | g | println;              // refused: `make` performs `Exn` here, unhandled

let pair = (1, make());        // stored, it performs nothing yet
let r = handle <5 | pair.1 { throw(m) => -1 };   // and performs here, handled
```

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
*is* the outcome type — see §11. A helper that only computes
with values — `at` above — stays an ordinary returning `fn`.

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
handler sits under every program (§8, "`IO`"), so a `mu` anywhere in `main`
is delimited by it; `exit` is not a captured continuation, and leaves from
anywhere.

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

`match` decomposes an enum by choosing the corresponding arm, and a variant
pattern binds exactly the payload its variant declares:

```sl
match shape {
    Point => 0,
    Circle(r) => (<(3, r) | mul | x => (x, r) | mul),
    Rect(w, h) => (<(w, h) | mul),
}
```

### Negative additive construction

`select` builds the consumer of any positive type by giving, for each shape
that type can take, a command. For an `enum` that is one arm per variant — the
negative additive:

```sl
fn k(return: i32) <- Color {
    select Color {
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

fn report(value: -i64, absent: -i64) <- Reading {
    select Reading {
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
while literals make the arm list ordered, first-match — that is `match`,
inside the arm.

An arm writes `=>`, exactly as a `match` arm does, because the arrow marks
which side of the mirror the scrutinee is on: **data flows forward into an
arm (`=>`); a demand reaches back into it (`<=`)**. A `select` matches data,
so its arms are `pattern => command`; a `mu` answers demands, so its arms are
`copattern <= command`; and a `match` writes whichever its scrutinee calls
for — `p => e` over a value, `.item(out) <= e` over a continuation. The
command an arm runs must be a cut `v | k` whose consumer is a visible
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

- **`select` answers data** — it builds the μ̃ family: the consumer of an
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
- **`match` — a branch table applied to a named scrutinee, on either side.**
  Over an enum value it takes data apart; over a continuation of a menu type
  (`k: -Config`) it takes the *request* apart: `.item(out) => e` binds the
  request's own continuation, and the arms are ordinary expressions —
  typically other requests. An arm has no guard: a test on what a pattern
  bound is a `match` inside the arm. And there is no `if`: a choice on a
  `Bool` is a `match` on it, `match c { True => …, False => … }`.

```sl
fn config() -> Config {
    mu Config {
        retries: out <= <3 | out>,
        name: out <= <"slant" | out>,
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
covers both, which is why one surface form — `match` — takes either apart.

A `data` value carries every field at once, which is what `⊗` means: the
positive product of its field types, one component each. `data
Direction { left: i32, right: i32 }` describes `+i32 ⊗ +i32`, and the surface
tuple `(a, b)` is the same connective written anonymously.

A product is taken apart by `match`/`select`, which binds every component, or
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
fn sum_pair(p: (i64, i64)) -> i64 { … }
command consume_pair | (k: (-i64 ; -i64)) { … }
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
  itself a choice, so a value of it is `::1(::0(v))`. A `select` over a
  choice answers each position exactly once; a `match` covers every one, or
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
  It is the consumer `select (A, B) { (a, b) => … }`
  builds, so a joint and a `form` value keep one runtime shape. It has no
  pattern form, for the reason a form has none. A joint value is one
  consumer, a closure over a single command, and does not hold the
  continuations it was built from; one built by `select` never had separate
  ones. A pattern `(k1 ; k2)` would have to invent them, as `form.field`
  would. What can be matched is what a joint is fed: `select (A, B)` takes
  the product apart.
- **`A -> B`** stays, as the spelling of `(dual(A) ; B)`.
- **The units are structural.** `(;)` is the type of a command; `(|)` has no
  value, is consumed by `select (|) {}`, and a `match` on one needs no arm;
  `(&)` is ⊤'s unique value. No unit has a name besides its spelling, and
  the core has all four: 1, 0, ⊤ and ⊥.

  `unit` is only a surface alias for `(,)`; it is not a fifth atomic type.

### Negative multiplicative construction

The consumer of a product is built the same way, by `select`. A product has
exactly one shape, so it has exactly one arm, and that arm binds every
component — which is what makes it multiplicative rather than additive: the
halves arrive together, in one command, sharing its context.

```sl
data Reading { value: i64, unit: String }

// dual(Reading) is `(-i64 ; -String)`: one consumer with both halves
fn show(out: -String) <- Reading {
    select Reading {
        Reading { value, unit } => <(<value | int_to_str, unit) | add | out>,
    }
}
```

A bare product needs no declaration; its shape is written as the type:

```sl
fn total(out: -i64) <- (+i64, +i64) {
    select (+i64, +i64) {
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
form is a consumer — μ̃ family — so `select` builds it; a menu is a μ[…]
value, so `mu` builds it. A menu's demand is one labelled request,
`.item(k)`; a form's is the whole record, `Report { … }`, whose type is the
form's dual.

```sl
fn printer(out: -i64) -> Report {
    select Report {
        Report { value, label } => { <label | println; <value | out> },
    }
}

<Report { value: 42, label: "answer" } | (<k | printer)>
```

`form` needs nothing new in the core: a form value is the `co(μ̃[…])` that
`select` over a product already builds, and its demand is that product
labelled — so the two meet by the existing labelled rule. What the
declaration adds is a *name* for the consumer side, so a signature can say
`-> Report` instead of spelling out `dual(…)`, and the fields of that
consumer can be named.

A form is always fed whole: there is no `form.field`. From `(-A ; -B)` no `-A`
can be extracted, though `(A, B)` yields its `A` — reading a field off a
record discards the others, and a form would instead have to *invent* them.
That is not a gap in the implementation but the shape of the connective, and
it is why a joint is not a record in any usable sense.

An atom is the degenerate product: one shape, one component. `select` covers
it too, and the arm's pattern is a plain binder that names the whole value:

```sl
fn show(out: -String) <- +i64 {
    select +i64 {
        n => <n | int_to_str | out>,
    }
}
```

That is the surface spelling of the core's value abstraction `μ̃x. c` — the
same binder `let` lowers to, written directly; `examples/duality/mu_tilde.sl` writes
that one co-term every way the surface offers. So `select` builds the consumer
of *any* positive type, with no exceptions: one arm per variant for a sum, one
arm binding every component for a product, one arm binding the value for an
atom.

### Traits

Impls dispatch on either side of the mirror: a `menu` or a `form` carries
one exactly as a `data` or an `enum` does — `impl Describe for Config`
with the method demanding `self.retries` — including bounded impls for
generic menus.

A bounded impl — `impl<+T: Display> Display for List<T>` — keys by the
declaration's name and covers every instantiation; its dictionary is
**constructed** at each use, the impl's global applied to one dictionary per
bound, read off the use's type arguments and built recursively:
`fmt` over a `List<List<i64>>` passes
`__dict_Display_List(__dict_Display_List(__dict_Display_i64))`. (v1 limit:
a *multi-method* trait's bounded impl cannot be constructed yet, and says
so.)

A `trait` names operations over an implicit `Self`; an `impl` gives them for a
type; a bound `<T: Show>` lets a generic use them. A method is a free function
overloaded on its first argument's type — `<x | show`, never `x.show()`:

```sl
trait Show { fn show(self: Self) -> String; }
impl Show for i64  { fn show(self: i64)  -> String { <self | int_to_str } }
impl Show for Bool { fn show(self: Bool) -> String { match self { True => "t", _ => "f" } } }

fn labelled<+T: Show>(x: T) -> String { (<("= ", <x | show) | add) }
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

An impl may be for an anonymous type too — a tuple, a choice or the unit. It
keys by its connective and width, and its components stand where a
declaration's type arguments do, so each bound is read off the component in
its position: `impl<+A: Display, +B: Display> Display for (A, B)`.

A method of several parameters takes them as one group, as any function
does, and `Self` is read off the components its parameters give that type.
Each component is checked against its parameter, and an integer literal
takes its width from the others, so with
`trait Combine { fn combine(self: Self, other: Self) -> Self; }`,
`<(1, x) | combine` for `x: i32` is `i32`'s `combine`.

A parameter may be bound by several traits, joined by `+`:

```
fn largest<+T: Ord + Display>(a: T, b: T) -> String {
    match (<(a, b) | gt) { True => <a | fmt, False => <b | fmt }
}
```

Each bound is its own dictionary, passed in the order written, so a second
bound costs what a second bounded parameter would. An impl is bound the same
way, `impl<+T: Ord + Display> Show for Pair<T>`. The `+` can only join inside
the bounds — the next parameter's sign comes after a `,` — and a second `:`
is refused with the spelling that was meant.

A method may be a `command`, taking continuations like any other; the
dispatch is unchanged. Method names are unique across traits in v1, bounds are
on positive type parameters, and associated types, default methods, and
supertraits are not yet provided.

### Effects and handlers

An `effect` names operations a computation may perform; a `handle` answers
them. Performing an operation suspends the computation and passes control to
the nearest enclosing handler with a matching clause. An operation is a demand, so its clause binds
the carried continuation the copattern way — after a colon, under any name
(`resume` by convention) — or omits it, for a clause that never resumes:

```sl
effect Exn    { fn throw(message: String) -> i64; }
effect Reader { fn config() -> i64; }
effect Choose { fn flip() -> Bool; }

// `checked_div`, `scaled` and `pick` perform them, as in `examples/effects/effects.sl`.
command main | (exit: i32) / {IO} {
    let safe = handle (<(10, 0) | checked_div) {
        throw(message) => -1,                       // never resumes: an exception
    };
    let reading = handle (<7 | scaled) {
        config(): resume => (<(<10 | resume, 1000) | add),  // resumes once
    };
    let all = handle pick() {
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

`handler Reader { clauses }` constructs a reusable handler value;
`handler [Reader, Other] { clauses }` lists several effects. The shorter
`handler { clauses }` infers the effects from the named operations.
`with h handle body` installs the value `h` around `body`:

```sl
effect Reader { fn config() -> i64; }
fn scaled(value: i64) -> i64 / {Reader} { <(value, config()) | mul }

command main | (exit: i32) / {IO} {
    let reader: Handler<i64, String, {Reader}, {}> = handler Reader {
        config(): resume => <10 | resume,
        return(value) => <value | to_string
    };
    <(with reader handle (<7 | scaled)) | println;
    <(with reader handle 42) | println;
    <0 | exit>
}
```

This prints `70` and `42`. `Handler<A, B, E, F>` is positive data: its body
produces `A`, its common answer is `B`, it discharges the concrete effects
in `E`, and its residual budget is `F`. The body's row must fit within
`E` plus `F`; installing a handler around a pure body or a subset of `E`
is valid. Clause effects must fit `F` too. All four arguments are invariant:
an annotation cannot grant a handler additional capabilities. Open handled
row tails do not grant unknown capabilities; installation subtracts only
the explicitly represented effects.

Construction installs nothing and runs no clause. Clauses run on installation
and operation demand; their effects therefore remain in the handler's type
when it is stored or returned. A `return` clause transforms even a pure body;
without one `A` and `B` coincide. Clause arity, common-answer typing and
whole-effect coverage are the same as for inline `handle`. An explicit
forwarding handler retains its forwarded effects in `F` rather than claiming
to discharge them in `E`. Reusing a handler does not cache bodies or answers.
Handlers can be stored in lists, selected at runtime and composed by nesting
installations; `examples/effects/handler_values.sl` demonstrates all three. Inline
`handle body { clauses }` uses the same clause-tree installation mechanism.

An operation is a free function, the dynamic mirror of a trait method: a
trait is an operation table keyed by a *type* and resolved statically — the
dictionary travels with the value — while an effect is an operation table
keyed by the *stack* and resolved dynamically: a handler is installed by
`handle` or `with h handle`, and its clauses bind the captured continuation, which no trait
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
fn map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
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
effect Reader<+T> { fn read() -> T; }
fn get<+T>() -> T / {Reader<T>} { read() }
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

A literal `handler Reader { clauses }` infers its arguments; an annotation
such as `Handler<i64, i64, {Reader<i64>}, {}>` constrains them. Merely changing
that annotation cannot change its capabilities. `examples/effects/generic_effects.sl`
demonstrates independent instantiations and stored handlers. The implementation
and additional checks are described in `docs/design-notes/generic-effects.md`.

#### `IO`: the effect the runtime handles

Reaching outside the program is an effect like any other, declared in the
prelude:

```sl
effect IO {
    fn write(text: String) -> (,);
    fn write_line(text: String) -> (,);
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
one of the program's own. A handler is an ordinary function taking the
computation it handles —

```sl
fn canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {fs::Fs, ..E} {
    handle <(,) | program {
        fs::read_file(path): resume => <::0("canned") | resume,
        _ => forward,
    }
}
```

— so `fs` exports `real` as a function, and a test installs its own. This
partial mock still requires an outer handler such as `fs::real` for `Fs`;
a self-contained mock answers all six operations and may discharge it. A
clause names its operation as a row names its effect, by path. An
operation answers with its outcome as a sum, `read_file(path) -> (String |
String)`, which the command then offers to its continuations: a clause runs
below its handler, so the continuations are activated by the command, under
the handler, not by the clause (`docs/design-notes/file-system-effect.md`).

The computation is written `fn { … }`, a lambda of no parameters: it is
`fn(_: (,)) { … }`. One that leaves through continuations of its own — a
program ending in `<0 | exit>` — has type `(;)`, a consumer, so it is not a
value to flow in; it is handed to a *command* as its exit, and
`fs::real_command` is `fs::real` for such a program:

```sl
command main | (exit: i32) / {IO} {
    let complain = select String { message => { <message | println; <1 | exit> } };
    <(,) | fs::real_command | (fn {
        <"input.txt" | fs::read | (select String { text => { <text | print; <0 | exit> } } & complain)>
    })>
}
```

An exit parameter preserves its supplied value's latent effects, just as
a function parameter does: `program: ((;) / {Fs, ..E})` describes what
running the program may perform. A `handle` whose body is `(;)` runs it as
a command under the handler. Passing the program does not itself run it.

**Latent rows describe effects at activation.** A function's row is charged
when it is applied; a menu's when an item is demanded; a form's or consumer's
when it is fed. Each uses the same effect-accounting rule, with a different
activation point.

`Delayed<T, E>` adds a separate forcing row before that activation. A cut
that only forwards an effectful function or menu as an answer preserves
its activation row; it does not perform it. In particular, the dual of a
rowed result retains the row as a requirement on the answer, not as an
effect of forwarding that answer. Double duality remains the identity.

A menu's demand row belongs to its type:

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
`command send<E>(value: i64) | (out: (-i64 / {..E})) / {..E} { <value | out> }`.
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
`fn drop(x: +i64) -> (;);` declares a consumer, and the cut `<42 | drop`
performs it. Routing to a chosen outcome needs nothing new
either, now that consumers are values — an operation takes them as
ordinary parameters, and the clause cuts into whichever it picks:

```sl
effect Judge { fn judge(n: i64, ok: -String, bad: -String) -> (;); }
…
judge(n, ok, bad) => match (<(n, 3) | gt) { True => <"big" | ok>, False => <"small" | bad> },
```

The clause runs below its handler, on the frames the continuations it is
handed were captured on, so its cut is an ordinary jump (§6).

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
function carries them in the same places — `fn emit<+T: Show>(out: -String)
<- i64 / {Log}` — because a bound constrains a type parameter and a row
describes what the body performs, neither of which depends on whether the
function returns a value or a consumer.

A bound on a consumer transformer is discharged by the **cut**, not by an
argument: in `fn emit<+T: Display>(out: -String) <- T`, nothing the call
receives mentions `T`, and `<42 | emit | s>` is what fixes it. So dictionary
solving waits until a declaration's body is fully checked — by then every
cut has spoken — and the same deferral gives a trait a second method
shape:

```sl
trait Describe { fn describe(self: Self) -> String; }   // receives Self
trait Deliver  { fn deliver(out: String) <- Self; }     // consumes Self
```

A returning method takes `self: Self` and dispatches on what it receives.
A consumer-transformer method takes no `self` — a consumer transformer's parameters are
all continuations — so its `Self` is the type it *consumes*, and dispatch
reads the value the cut sends: `<42 | deliver | s>` finds the `i64` impl,
`<True | deliver | s>` the `Bool` one. Both shapes resolve statically, and a
bound forwards through either.

### Polymorphism

Two forms, one discipline. A declaration may take type parameters —
`fn id<+T>(x: T) -> T` — which are rigid inside their own body and
instantiated afresh at every call. And a `let` generalizes, under the
**value restriction**: only when its right-hand side is a syntactic value —
a literal, a `fn`, a `select`, a constructor, record, tuple, or box of
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
declaration, `fn id<+T>(x: T) -> T`.

Anything that computes stays monomorphic — `mu { k <= c }` above all, and every
application. A value ran nothing, so no two instantiations can disagree
about anything that happened; a computation may have captured its
continuation, and generalizing that is the classical unsoundness (the
Harper–Lillibridge counterexample is a `mu` returning a polymorphic
function; with continuations that resume, it would execute). When the
per-use behaviour is wanted, write it: `fn { mu { k <= … } }` is a
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

Here "shifts" means polarity-shifting type wrappers, not composable control
capture. The library operation `control::shift` in §6 is unrelated.

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
`Lazy<T>` has an empty demand row. `Delayed<T, E>` instead annotates an
implicit computation and accepts only negative `T`. They are not literal
duals: `dual(Lazy<T, E>)` is the menu's request type, carrying a continuation
for its answer, not a delayed computation. For negative `T`,
`lazy::of_delayed` and `lazy::to_delayed` convert between the two interfaces
without running the computation at conversion time. Both preserve
call-by-name: repeated demands repeat construction. Neither caches results.
`examples/laziness/delayed_and_lazy.sl` contrasts the interfaces and their handlers.

So a consumer travels bare everywhere a value does: an enum payload
(`Refutes(-i64)`), a record field, a `fn` value parameter — passing a
continuation is an ordinary application, `<k | handle`. `dual` is an
involution on the nose: `-(-T)` *is* `T`, and double-negation elimination
is the identity function.

```sl
fn dne<+T>(t: -(-T)) -> T { t }

<42 | dne   // 42: -(-i64) and +i64 are one type
```

One orientation rule remains, and it is load-bearing: **the left of `|` is
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
| a lambda's parameter and result: `fn(x) { … }` | how the value is used fixes the parameter's polarity by the end of the declaration    |
| a `mu`'s produced type: `mu { k <= … }`        | the arm hands `k` to a slot whose type is declared, or cuts a value against it          |
| a `select`'s type: `select { … }`              | an arm's pattern names it, or the enclosing consumer transformer already said what it consumes |
| a type's sign: `x: +i64`, `k: -i64`             | it agrees with the position — see below                                                 |

**A sign is omitted where the position implies it.** The table in §4 has a
diagonal: a value parameter is positive, a continuation row is negative, and
the type after `<-` is positive. On the diagonal the sign says nothing the
position had not already said, so it is left out — `command nth<+T>(xs:
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
fn code(return: i32) <- Color {
    select {
        Red => <0 | return>,
        Green => <1 | return>,
    }
}

// Nothing in the arm names a type, but `<- i64` did.
fn twice(out: i64) <- i64 {
    select {
        n => <(n, 2) | mul | out>,
    }
}
```

What is left is what nothing else says. `select { n => <n | k> }` bound to
a `let`, outside any consumer transformer, is rejected: no arm names a type and no
declaration supplied one, so it is written.

`(k1 ; k2)` builds a joint from separate consumers, but does not make them
progress independently. There is no way to feed one half at a time: a joint
is supplied whole, and has no destructuring pattern. Sequencing returning
sinks uses ordinary functions of type `A -> (,)`, not consumers `-A`.
A `select` arm must be a command, never a unit-valued returning sink; a cut
does not return to the statement following it.

## 9. Entry point and exit

A program is a command, so its entry point is a `command`. It takes no values
and exactly one continuation — the exit status:

```sl
command main | (exit: i32) / {IO} {
    <"Hello, Slant!" | println;
    <0 | exit>
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
command main | (exit: i32) / {IO} {
    let complain = select String { message => { <message | println; <1 | exit> } };
    <(,) | fs::real_command | (fn {
        <"input.txt" | fs::read | (select String { text => { <text | print; <0 | exit> } } & complain)>
    })>
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

The library has two layers, and both are ordinary Slant source that goes
through the same pipeline as user code.

**The prelude** (`crates/slc-driver/src/prelude.sl`) is what every program
sees unasked: the `effect IO` the runtime handles, and the
**`Display` trait** (`fn fmt(self: Self) -> String`, user-facing formatting
as in Rust) with impls for `i64`, `String`, `Bool`, and the unit, tuples and
choices up to eight components, rendered as they are written — and
`to_string<T: Display>`; `enum Bool { False, True }`, the type every yes-or-no
answer has; arithmetic and comparison as the traits `Add`, `Sub`, `Mul`,
`Div`, `Rem`, `Neg`, `Eq` and `Ord`, with impls for the base types; `index`,
a `String`'s character at a position; and `not`, which negates a `Bool`,
since there is no `!`. A
program's own declaration of a prelude name shadows it.

**The stdlib** (`crates/slc-driver/src/stdlib/`) is one module per file,
appended after the prelude, and nothing in it is in scope until named: a
module is reached by its path, `list::length`, or a name is brought in bare
with `use`. Each file supplies the body of the module named by its file stem,
so `stdlib/list.sl` is loaded as `mod list { … }`; the source does not repeat
that wrapper. `prelude.sl` is the root-scope exception. Each module marks what
it offers `pub`; the rest is its own.

| module | what it offers |
|---|---|
| `list` | `List<T>`, `length`, `append`, `map`, the outcome-offering `command nth` — and `impl<+T: Display> Display for List<T>`, which lives with the type and is found from anywhere (`[1, 2, 3]`) |
| `string` | `Builder`, a persistent string builder expressed as a `menu`; `new`, `push<T: Display>` and its `append` and `finish` items |
| `option`, `either` | `Option<T>` with `unwrap_or`; `Either<L, R>`, `Left` or `Right` with neither meaning success. Either/or outcomes are additive, so they are enums whose consumers are `select`s — a `form` would want every field at once |
| `num` | `min`, `max`, `abs` |
| `stream` | `Stream<T>`, the coinductive mirror of `List`, with `repeat`, `count_from`, `iterate`, `unfold`, `map`, `zip`, `drop`, and `take` bridging back to data, since an infinite structure cannot print whole and showing `<(s, n) | take` is the honest form |
| `seq` | `Seq<T>`, the finite codata sequence between the two (below) |
| `lazy` | `Lazy<T, E>`, the explicit by-name thunk for either polarity; `of_delayed` and `to_delayed` convert negative-result computations to and from `Delayed<T, E>` |
| `fs` | files: `read`, `write`, `open`, `read_line`, `close`, `exists` — commands offering each outcome to its own continuation, performing the `Fs` effect — and `real`, the handler that answers it from the disk |
| `control` | `Shift<A, R, E>`, `shift` and the thunk-taking `reset`: typed, multi-shot composable capture with a positive answer type and explicit residual effects |
| `trace` | one **tap**, `command tap(label, x) \| (k)`, which logs what passes through and forwards it: `<("answer", 42) \| trace::tap \| out>` |

String assembly is a library operation, not a new literal or variadic syntax.
`string::new()` returns a `string::Builder`, whose `append` menu item answers
with a function from a `String` to the next builder, while `finish` answers
with the accumulated `String`. `string::push` renders any `Display` value and
uses `append`. Each builder is persistent: its menu arms close over one
accumulated value, so adding returns a new state and a shared prefix can safely
branch. [`examples/programs/string_builder.sl`](examples/programs/string_builder.sl)
shows the complete program.

The program's text comes first in the combined source, so its spans and
line numbers are untouched; a diagnostic inside the library names its unit,
`list.sl:53:57`. Only the units a program reaches are loaded: the prelude always, and each
module named by a path or a `use`, with the modules those name in turn — so a
program that touches no module is checked against the prelude alone. `examples/basics/stdlib.sl` draws on the second layer only.

**`Seq<T>` is the one that pays for menus in ordinary code.** `List` is
data and `Stream` is codata that never ends; a `Seq` is a menu whose single
item answers *whether* there is more, so the recursion lives in the codata
and the branching in the data:

```sl
enum Step<+T, E> { Done, Yield(T, Seq<T, ..E>) }
menu Seq<+T, E> / {..E} { next: Step<T, ..E> }
```

It is produced a step at a time and only as far as it is demanded, which is
what neither neighbour can do — so `seq::filter` over an infinite source is a
terminating program as long as something downstream stops asking:

```sl
<1 | stream::count_from | seq::of_stream | s => (odd, s) | seq::filter | s => (s, 4) | seq::take  // [1, 3, 5, 7]
```

Beside it: `seq::of_list`/`seq::to_list` and `seq::of_stream` for the bridges,
`seq::map`, `seq::filter`, `seq::take`, and `seq::take_while`, which cuts a stream
where a value stops passing and therefore answers a `Seq` — the type saying
what the function does. `examples/laziness/seq.sl` runs all of it. A `Seq` carries
the row its steps perform when demanded, `Seq<T, ..E>`, so `seq::map` with an
effectful function answers at once and its effects happen where the steps are
demanded — under whatever handler is around `seq::to_list`. There is no
`impl Display for Seq`, for the reason `Stream` has none: showing one is
`seq::to_list`, or `seq::take` first if it may not end.

**A stdlib helper that takes both values and continuations is a
`command`.** That is what the declaration square calls the shape, and the
header says it: the value group before the `|`, the menu of exits after. It
could instead be a returning `fn` returning `-T` — the same type, since
`A → ⊥` *is* `-A`, and a consumer transformer cannot do it because its one
parameter group *is* its row — but that spelling says the shape only in the
return position, and it makes the caller build the consumer before cutting
into it rather than write the call every other call is written as. Two
combinators had it and are gone: `then(f, k)`, because composing a function
with a continuation is `f | k>`, and `defaulting(fallback, k)`, because a row
slot wants a consumer and `select String { m => <fallback | k> }` is the
consumer — the combinator only hid the arm. The `<- A` form remains the
natural spelling for a consumer transformer whose inputs are all
continuations.

**Builtins** are what the language cannot express — I/O, arithmetic on
machine numbers, string internals — and they follow the same rule the
language does: **a builtin whose outcome
is a single value is an ordinary function; a builtin whose outcome is not —
it can fail, or find nothing — takes continuations and denotes a command.**
The value arguments come first, then one continuation per outcome, and exactly
one of them is activated.

| Builtin      | Values                                           | Outcomes                                            |
|--------------|--------------------------------------------------|-----------------------------------------------------|
| `parse_int`  | `text: +String`                                  | `ok: -i64`, `invalid: -String`, `overflow: -String` |
| `char_at`    | `text: +String`, `index: +i64`                   | `ok: -char`, `out_of_range: -String`                |
| `find_char`  | `text: +String`, `from: +i64`, `character: +i64` | `found: -i64`, `absent: -String`                    |

Every failure continuation receives a `+String` describing what happened, so
it composes with an error consumer a program already has.

```sl
<"input.json" | fs::read | (
    select String { source => <source | parse_json | report> }
    & complain
)>
```

A consumer per outcome is what `select` builds, so an outcome's handler can be
written where it is passed rather than declared elsewhere.

Everything else is a function: `println`, `print`, and `format`; arithmetic and
comparison; `str_len`, `str_concat`, `int_to_str`, `str_eq`, `substring`;
`is_digit`, `is_ws`, `skip_ws`, `skip_digits`.

**Files are the `fs` module's**, not builtins a program has unasked:
`fs::read`, `fs::write`, `fs::open`, `fs::read_line` offer
their outcomes as above, and `fs::close` spends a handle so a later read
through it fails, `fs::exists` answers a `Bool`. Each performs the `Fs`
effect, so the code that calls them runs under a handler: `fs::real` answers
from the disk through the runtime's primitives, `__read_file` and its
siblings, which are what the language cannot express (see "`IO`: the effect
the runtime handles").

A handle is a value of its own base type, `File`, produced only by
`fs::open` — so nothing else closes a file or reads a line. A read after
`fs::close` is a runtime error.

Composing a close onto an exit closes the file on paths routed through
that wrapper. Shadow `exit` where the handle comes into scope:

```sl
let file = mu { k <= <path | fs::open | (k & complain)> };
let exit = select i32 { status => { <file | fs::close; <status | exit> } };
```

The arm's `exit` is the outer one. A later direct `| exit>` uses the wrapper,
but a consumer created earlier still captures the old exit. Shadowing does
not rewrite closures or captured continuations. Build failure consumers
used after acquisition around the wrapped exit as well;
`examples/programs/file_io.sl` routes its post-acquisition success and failure paths
this way. Unrestricted control supplies no automatic resource guarantee.

Two failures stay fatal rather than becoming outcomes: an out-of-range
`<(s, i) | index` and a division by zero. `index` and `div` are plain
functions, which have nowhere to put a continuation, and — as in Rust, where
`v[i]` panics while `v.get(i)` does not — they report a bug in the program
rather than a case it was meant to handle. The checked forms are the
`command`-shaped builtins above, `char_at` among them.

A helper of your own that always ends in a cut is annotated `-> (;)`: it never
returns, so it may stand where a consumer is expected.

## Literals

A lambda whose body ends in a cut produces nothing, so it *is* a consumer:
`fn(message: +String) -> (;) { … }` has type `-String`, and may be written
wherever a consumer of a `String` is expected. `A → ⊥` and `-A` are the same
type, not two that convert, so a continuation parameter may be annotated
either way.

The `-> (;)` may be omitted — the body decides the type — but the examples
write it, because a consumer literal is worth reading as one at a glance.

`A -> B` is `(-A ; B)`, which is why this works: `A -> (;)` is `(-A ; (;))`,
and `(;)` is the unit of `;`. The same identity gives the dual: `dual(A -> B)`
is `(A, -B)`,
an argument together with a continuation for the result — a *call stack*. So
`<v | f` delivered to `k` and the cut of `f` against `(v, k)` are the same interaction,
and a consumer of a function is an ordinary value of that product type.

A cut is well typed exactly when its two sides are dual. Which side is
written negatively is not itself the question: `v | k` sends `v` to something
that consumes it, and for a function that something is a call stack.

An integer literal takes the integer type its port requires — `<0 | exit>`
sends an `i32` — and is `+i64` when nothing constrains it. The integer
primitives are `i8`, `i32`, `i64`, `u8`, `u32`, and `u64`; a floating-point
literal similarly takes `f32` or `f64` from its port and is `+f64` when
unconstrained. Every other value must match its port exactly: there is no
implicit widening or narrowing of a value that is not a literal.

The numeric primitives have the `Display`, `Add`, `Sub`, `Mul`, `Div`, `Rem`,
`Eq`, and `Ord` implementations supplied by the prelude; signed integers and
floats also have `Neg`. A floating-point literal does not coerce to an integer
type, and an integer literal does not coerce to a floating-point type. Numeric
patterns, including inclusive ranges, take the numeric type of their
scrutinee. A string literal is `+String`, a character literal is
`+char`. `True` and `False` are not literals but the
variants of the prelude's `enum Bool`. `(,)` is the
unit value, of type `(,)`; an empty block is the same.

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
so on. Within one phase, diagnostics are source-ordered. Every diagnostic
names its `line:column` and quotes the source it is about — a lex or parse
error as a checker's does: `parse error: there is no `+` operator … (at 3:8
`+`)`. A syntax error's span can run to the end of the file, as an
unterminated string's does, so only its first line is quoted; and one with no
extent, at the end of input, names no place rather than a wrong one.

Every source unit — the program's file, each module file, each library
file (§10) — is lexed on its own, and a program's files have their syntax
checked on their own. Read as one text, a string the program left open
would close on the prelude's first quote, and a brace on its last, and the
error would be reported in a file the author did not write.

Runtime failures are not compiler diagnostics. They are reported after
evaluation begins and do not participate in this precedence order.

`slc check <file.sl>...` applies the phases and stops: it reports what `run`
would report before evaluating, and evaluates nothing, so a program that
would not stop can still be checked. Each diagnostic is prefixed with its
file, and one failing file fails the command. A file that declares no `main`
is a library and checks; a `main` of the wrong shape is refused as `run`
refuses it (§9).

Some slips get a message of their own rather than the mismatch they cause. A
handler clause binding the wrong number of parameters reports the operation's
arity and the number bound; a clause returning a different type reports the
handler's answer type. An incomplete effect handler lists the missing
operations and suggests a final `_ => forward` when forwarding is intended.
A function closed with `>` — `<42 | resume>` in a handler's clause, where
`<42 | resume` was meant — is reported as a function applied by leaving the
`>` off, instead of as a value meeting the argument-and-continuation pair a
function takes by a cut. A handler clause naming no operation of any
effect — `fs::nope(path): resume => …` — is refused by name, where it would
otherwise be ignored and leave its effect reported as unhandled.

## 10. Modules

A `mod` is a named scope of declarations, `::` reaches into it, and `use`
brings one name into scope:

```sl
mod geometry {
    pub enum Shape { Circle(i64), Rect(i64, i64) }

    fn squared(n: i64) -> i64 { <(n, n) | mul }   // private: the module's own

    pub fn area(s: Shape) -> i64 { … }    // its own names are bare here
}

use geometry::area;

command main | (exit: i32) / {IO} {
    <geometry::Shape::Circle(5) | area | println;
    <0 | exit>
}
```

**A declaration inside a module is private unless it is `pub`.** Private
means reachable by that module and the modules nested inside it, and nowhere
else — so a module's helpers are not part of its surface. A declaration in
no module is visible everywhere, which is what lets the prelude be the
prelude and leaves a single-file program unaffected.

The rule is enforced after flattening, where both halves are known: a
reference is a qualified name, and the declaration it sits in carries the
module it was written in. A reference reaches the longest declared prefix of
the path it names, so `geometry::Shape::Circle` is refused when `Shape` is
private, not only when some `Circle` is.

The library's modules are declared in their own source units, appended
after the program. Every library file except `prelude.sl` is implicitly
wrapped in a module named after its file stem. Imports are scoped to the unit
that wrote them: a
library file's `use Enum::*;` pins names in that file only, and a program's
imports never reach into the library. The root scope, though, is one scope
over every unit, so a library unit imports names only inside its `mod` —
at its top level a `use` may be a variant import, which is per-unit, and
nothing else. A program's `mod` of a library module's name shadows it
whole, as its `fn` shadows a prelude function. `use list;` — naming a
module already reachable at the root — is allowed, so a program can say
what it draws on.

`use m::*;` brings every `pub` member of module `m` in bare — a glob. It is
the weakest way a name arrives: an explicit `use m::f;` and the importing
module's own declarations both win over it. Two globs may bring the same
name, and that is not an error until the name is used — `use list::*; use
seq::*;` is fine, and a bare `map` after it says it could be either and asks
for the one you mean. A glob over an enum, `use list::List::*;`, still
brings its variants, as before.

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

Visibility follows the private-by-default rule above, and `main` must be
declared at the root — a `main` inside a module is `m::main`, which the entry
point does not accept.

### A module in a file of its own

A `mod` with no body names a file that holds the module's body:

```sl
mod geometry;          // the declarations of `geometry` are in geometry.sl
pub mod report;        // public, as `pub mod report { … }` is
```

The file holds the declarations themselves, with no `mod geometry { … }`
around them: the name is given once, where the module is declared. Nothing
else about the module changes — privacy, paths, `use` and globs are as
above — because a module in a file is a module.

The directory tree is the module tree:

| declared in                  | `mod name;` is         |
|------------------------------|------------------------|
| the program, `dir/main.sl`   | `dir/name.sl`          |
| a module file, `dir/m.sl`    | `dir/m/name.sl`        |
| inline, inside `mod a { … }` | one `a/` further down  |

There is no search path and no way to name a file elsewhere: a program is
the files under its own directory. A `mod name;` whose file is missing is
reported at the declaration with the path that was tried, and a file reached
twice is refused rather than declared twice.

A module file is a *source unit*, as each library file is. It is lexed on
its own — so nothing one file leaves open can close in another — and its
syntax is checked on its own; then its tokens take the place of the `;`,
between a `{` and a `}`, and the parser reads one ordinary program. That is
why a `menu` declared in one file is known in the others (§7), and why
imports behave as they do across files: a variant import is scoped to the
unit that wrote it, so a file's `use Shape::*;` pins bare names for that
file alone. A diagnostic in the program's own file is `line:column`; in any
other unit it names the file, `src/geometry.sl:4:9`.

`slc run` and `slc check` take the program — the root. `slc fmt` is per file,
and formats a module file like any other. `docs/design-notes/file-modules.md`
records the alternatives.

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
              | (t₁ ⊗ … ⊗ tₙ)          tuple, n ≥ 2
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
              | (A, …, A) | (A ; … ; A)   multiplicatives, any number of components
              | (,) | (;)             their units: no components
              | (A | … | A) | (A & … & A) additives, any number of components
              | (|) | (&)             their units
              | (A -> A)              function: (dual(A) ; A)
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
continuation is activated by reinstating its stack, down to the nearest
handler it shares with the running one, which is why it outlives its `mu`
and can be used more than once. `select` branches stay unevaluated
until activation chooses one. A run is bounded only by memory; `slc run
--fuel N` caps it at `N` machine steps, turning divergence into an error.

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
Resuming copies the captured slice onto the running stack, so it costs the
slice's frames, not the stack's depth. A cut into a co-variable that only
forwards — one nothing binds, or one holding the very stack running now —
pushes no frame, so a loop whose body ends in such a cut runs in constant
space, and so does a handler that resumes in tail position around it.

A handler's prompt carries an id, fresh at each installation and kept by a
resumption's copy. A jump walks the running stack from the top: at the first
frame the captured stack shares, the captured stack replaces it; at the
first prompt, the captured frames above that prompt go on it, or, when the
captured stack holds no prompt with its id, the jump is the error of §6.
Every frame records the depth beneath it, so the two stacks are lined up
without walking either to the bottom.

### Printed form

The grammar above is also the core's printed form: the compiler prints terms,
co-terms, commands, and types in exactly this syntax, and reads them back
unchanged. Printing is therefore a faithful view of the IR rather than an
approximation of it, and a printed declaration can be compared, stored, or
re-parsed without loss.

Types print in the surface's own spelling — `(A, B)`, `(A ; B)`, `(A & B)`,
`(A | B)`, their units `(,)`, `(;)`, `(&)` and `(|)`, and `dual(A)` — so a
diagnostic shows a type the way a program writes it. A two-component `;`
whose first half is a consumer prints as the function it is, `(A -> B)`.
Only the core's terms keep their own notation.

### Reduction

```text
⟨ μα. c ∥ e ⟩                → c[e/α]              μ
⟨ v ∥ μ̃x. c ⟩                → c[v/x]              μ̃ — the binder
⟨ λx. t ∥ v · e ⟩            → ⟨ v ∥ μ̃x. ⟨ t ∥ e ⟩ ⟩  → — application
⟨ co(e′) ∥ v · e ⟩           → ⟨ v ∥ e′ ⟩           apply the reified consumer
⟨ (t₀ ⊗ … ⊗ tₙ) ∥ prj:i ⟩    → tᵢ                  projection
⟨ L(v₁ ⊗ …) ∥ μ̃[M; … L(x…). c …] ⟩ → c[vᵢ/xᵢ]       labelled
⟨ μ[… .d(α). c …] ∥ .d(e) ⟩  → c[e/α]              copattern
⟨ co(.d(e)) ∥ μ̃[M; … .d(x). c …] ⟩ → c[co(e)/x]     co-labelled
⟨ (v₁ ⊗ … ⊗ vₙ) ∥ μ̃(x₁, …, xₙ). c ⟩ → c[vᵢ/xᵢ]    product
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
`e`. Application uses a fresh result continuation; parameter groups pack
into one product of values and, for commands, one bundle of exits. By-name
positions wrap negative computations in `λ$delay. ⟦e⟧` before passing or
storing them. Positive arguments are evaluated before computed stages.

| Construct | Surface | Core |
|---|---|---|
| `expr.literal` | `42`, `"s"`, `'c'` | a constant variable (`$int_42`, `$str_"s"`, …) |
| `expr.ident` | `x` | `x` |
| `expr.enum` | `Color::Red`, `Shape::Circle(r)` | `Color::Red(unit)`, `Shape::Circle(⟦r⟧)` — several payload values pack into one tensor |
| `expr.call` | `f()`, primitive or trait compatibility calls | unit for a nullary call; otherwise group arguments as the signature declares, then apply; primitives retain their internal curried encoding |
| `expr.lambda` | `fn(x: +A) -> B { e }` | `λx. ⟦e⟧`. A stage `x => e` is `fn(x) { e }`, and a stage `k <= e` followed by `rest>` is the closing consumer `<rest> \| fn(k) { e }` |
| `expr.pair` | `(a, b, …)`, `(,)` | the tuple `(⟦a⟧ ⊗ ⟦b⟧ ⊗ …)`; `(,)` is `unit` |
| `expr.inject` | `::i(v)` | `\|i(⟦v⟧)` — the position is the whole label, whatever the sum |
| `expr.let` | `let x = v; e` | `μlet. ⟨ ⟦v⟧ ∥ μ̃x. ⟨ ⟦e⟧ ∥ let ⟩ ⟩` — a binder is `μ̃`, the value abstraction. A binder that is a pattern is the one-arm `match` it abbreviates: `μ__match. ⟨ ⟦v⟧ ∥ μ̃p. ⟨⟦e⟧ ∥ __match⟩ ⟩`, over the same branch table `expr.match` builds. A parameter pattern binds the group to one name and destructures it the same way |
| `expr.block` | `{ e₁; e₂ }` | `μ__seqᵢ. ⟨ ⟦e₁⟧ ∥ μ̃__discarded. ⟨ ⟦e₂⟧ ∥ __seqᵢ ⟩ ⟩` — both generated binders are fresh against the expressions they enclose |
| `expr.flow` | `<v \| k>`, and every other chain | `μ__cut. ⟨ ⟦v⟧ ∥ k ⟩` for a named consumer; for a computed one, first bind the by-name input to a fresh `saved`, then evaluate the consumer and apply it to `saved`. The cut has no result. Open chains fold grouped applications; composition wraps that fold in a λ. A command takes its value group and exit bundle together; yielding exits compose returning callbacks into a fresh captured result continuation (§5) |
| `expr.mu` | `mu A { k <= e }` | `μk. ⟨ ⟦e⟧ ∥ k ⟩` — the captured continuation, not a declared parameter; the type in front is what the expression produces |
| `expr.match` | `match s { p => e, … }` | a match the core can express — every arm a shape (variant, record, tuple, request, or one whole-value binder), components binders or nested products, no duplicates — is a genuine cut: `μ__match. ⟨ ⟦s⟧ ∥ μ̃[T; L(x…). ⟨⟦e⟧ ∥ __match⟩ \| … ] ⟩` (`μ̃(x…)`/`μ̃x` for a product/atom). Anything order-sensitive — literals, or-patterns, a default among labelled arms — falls back to `__match_dispatch(⟦s⟧, arm₁, …)`, each arm `__match_arm(descriptor ⊗ λ__match_arg. ⟦e⟧)` |
| `expr.data` | `S { f: v, g: w }` | `S((⟦v⟧ ⊗ ⟦w⟧))` — the declaration's name labelling the tuple of its fields, the same shape a variant has |
| `expr.select` | `select T { p => c, … }` | `co(μ̃[T; L(x…). ⟦c⟧ … ])` for a labelled type — one branch per shape, the pattern's binders naming that shape's components — and `co(μ̃[T])` when it has no shapes; `co(μ̃(x…). ⟦c⟧)` for a product, and `co(μ̃x. ⟦c⟧)` for an atom, whose one binder takes the whole value |
| `expr.comatch` | `mu T { item: k <= c, … }` | `μ[T; .T::item(k). ⟦c⟧ | …]` — the copattern form of `mu`: a menu value, one branch per demand. Nested copatterns group by their outer destructor: the branch binds `__k`, and its body cuts the inner menu against it |
| `expr.request` | `.item(k)` | `co(.M::item(k))` for a named continuation; any other expression is bound first, then named. A demand `cfg.item` is `μ__ask. ⟨ ⟦cfg⟧ ∥ .M::item(__ask) ⟩` |
| `decl.menu` | `menu M { item: A, … }` | no term of its own: `mu M { … }` builds the `μ[…]`, and its items name the `.M::item(e)` requests |
| `decl.form` | `form F { field: A, … }` | no term of its own: `select F` builds `co(μ̃[F; F(x…). ⟦c⟧])`, and `F { … }` builds the demand `F(⟦v⟧ ⊗ …)` it consumes |
| `expr.consumer_argument` | `<k \| f` — a consumer as an argument | `⟦k⟧` — a consumer is a value; nothing to coerce |
| `expr.handler` | `handler E { clauses }` | a labelled clause tree containing operation closures and the return closure, defaulting to identity; unmatched operations forward outward |
| `expr.with_handler` | `with h handle body` | runtime handler installation with `⟦h⟧` and a thunk of `body`; inline `handle` builds the same clause tree |
| `decl.fn.returning` | `fn f(x: +A) -> B { e }` | `λx. ⟦e⟧` |
| `decl.fn.transformer` | `fn f(k: -A) <- B { e }` | `λk. ⟦e⟧` |
| `decl.mu` | `command f(x: +A) \| (k: -B) { e }` | `λx. λk. ⟦e⟧` |
| `decl.const` | `const C: +A = v;` | `⟦v⟧` |
| `decl.enum` | `enum E { V }` | one global per variant: `E::V = E::V(unit)` |
| `decl.data` | `data S { … }` | no term; the declaration is a type |

Block sequencing returns through a bound, fresh continuation in the core.
When that continuation is used only for the final return, compilation
replaces that return with the machine's `Forward` instruction: deliver to
the current stack, including the stack reinstated by a handler resumption.
This keeps tail-resuming loops constant-space without relying on an unbound
`__tail` name. Escaping or otherwise used continuations retain ordinary
capture semantics.

Parameter groups are nested in declaration order. Each group becomes a λ
binder followed by destructuring; arguments keep the order written within
the group. This is not partial application of individual source parameters.

### Surface-to-core coverage

| Core construct | Surface representation |
|---|---|
| `x`, `λx. t` | identifiers, functions, lambdas, and declared continuation parameters (`fn … <- …`, a `command`’s row) |
| `μα. c` | local `mu` expression, a flow that closes against a named consumer, and the lowering of `let`, blocks, and applications |
| `t ⊗ t` | tuple literals, `data` literals, `(A, B)` values |
| `L(t)` | `enum` values and `data` values — a labelled product — and choices `::i(v)`, labelled by their position `\|i` |
| `μ[M; .d(α). c \| …]` | `mu` over a `menu` — the copattern form |
| `.d(e)` | a demand `cfg.item`, and the consumer inside a request literal `.item(k)` |
| `co(e)` | `select`, and every consumer in value position — a reified co-term, and a `form` value, `(k1 ; k2)` included |
| `α` | the consumer named on the right of a cut, `<v \| k>` |
| `v · e` | application, `<a \| f`, and a cut whose consumer is computed rather than named, which applies the resulting consumer after binding the input |
| `μ̃x. c` | every binder: `let`, a discarded block expression; written directly as `select +A { x => c }` |
| `μ̃[T; …]`, `μ̃[T]` | `select` over an `enum`, a `data` or a sum `(A \| B)`, including `select (\|) {}` |
| `μ̃(x…)` | `select` over a bare product |
| `prj:i` | `base.i` (tuple) and `base.field` (a record), the field resolved to its index from the base type |

### Classical control

The core is classical, so the classical laws are ordinary programs. Negation
is a consumer — `¬A` is `-A`, since `A → ⊥` and `-A` are one type. Double
negation is an involution, so its elimination is the identity. Excluded
middle uses `mu`, which hands out the continuation of its expression:

```sl
fn dne<+T>(value: -(-T)) -> T { value }

// A ⊕ ¬A: answer with the refutation, which is the continuation in disguise.
fn lem() -> Choice {
    mu { k <=
        <Choice::Refutes(select i64 { a => <Choice::Holds(a) | k> }) | k>
    }
}
```

`examples/duality/classical.sl` runs both. The types above go through the shifts of
§8 — `-(-i64)` *is* `+i64`: `dne` is the identity, and `<42 | dne` is `42`.
Involution does not make an integer executable: putting a positive atom on
the consumer side of a cut is rejected after inference, whether named or
computed.

A captured continuation is a value with no expiry: the evaluator is an
abstract machine whose continuation is an explicit frame stack, and `mu`
captures by reifying it. Activating `k` *reinstates* that stack — after the
`mu` has answered, from however deep, as many times as it is reached, down
to the nearest handler the two stacks share (§6) — so
taking `lem()`'s offer re-enters the very `match` that already received
`Refutes`, which this time holds.

## 12. Error continuations

Fallible operations receive their result continuations directly. For example,
a parse operation receives both a success continuation and an error
continuation:

```sl
let parsed = select +String { value => { <("parsed: ", value) | add | println; <0 | exit> } };
let failed = select +String { message => { <("error: ", message) | add | println; <1 | exit> } };
<source | parse_json | (parsed & failed)>
```

No result wrapper is needed, and nothing carries a success value alongside an
error value: the continuation that is activated *is* the outcome.

**A row of continuations is already the outcome type.** The consumer of
`A ⊕ B` is a consumer of `A` together with a consumer of `B`, so declaring an
`enum` of outcomes and sending it to a single continuation adds a wrapper
without adding information — and it costs something, because the row can say
what a single continuation cannot: which outcomes each operation actually has.
In `examples/programs/json_parser.sl` every parser takes `failed`, but only the
top-level one takes `parsed`, so no inner parser can report success by
mistake. Keep an `enum` for data that a program *holds*; outcomes that a
program *reaches* are a row.
