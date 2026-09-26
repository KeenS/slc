Part of the [language design](../../DESIGN.md).

## 4. Function orientation and evaluation polarity

A **returning function** uses `->` and produces a value:

```sl
func plus(x: +i32, y: +i32) -> i32 {
    <(x, y) | add
}
```

A **consumer transformer** consumes continuations and produces a continuation. It is
written with the reverse arrow:

```sl
enum Status { Ok(i64), Failed(i64) }

func report(success: -i64, failure: -i64) <- Status {
    mu Status {
        Ok(code) => <code | success>,
        Failed(code) => <code | failure>,
    }
}
```

The arrows identify the direction of the cut:

- `fn(value_params) -> Output` consumes values and produces a value.
- `fn(continuation_params) <- ContinuationType` consumes continuations and
  produces a continuation.

There is one `func` declaration form. The old `+fn` and `-fn` prefixes do not
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
| **positive type** | data arrives: `func f(x: +i64)` | the type after `<-`: `func config() <- Request` |
| **negative type** | a consumer arrives: `func f(note: -String)` | the row of a `proc`: `proc f \| (k: -i64)` |

The diagonal is the ordinary reading — data in, control out. The other two are
what polarity buys:

- A **negative type in argument position** is a consumer received as data. A
  returning `func` may take one, because it returns rather than ending in a cut,
  so it promises nothing about consuming it; a `proc` splits its parameters by
  polarity, so a consumer there belongs in the continuation group instead.
- A **positive type in continuation position** is the type after `<-`. A
  continuation is named by the type it consumes, so `func config() <- Request`
  writes a positive type and produces its consumer.

Consuming codata reverses a cut's usual sides: a provider is negative, so what
consumes it is its dual — the positive request. In
`<Request::Retries(answer) | provider>` the provider is the consumer and the
request is the value.

`examples/duality/polarity.sl` writes all four; `examples/errors/polarity_error.sl` writes
the two a `proc` rejects.

A type the checker has not solved yet still has a polarity once it meets a
generic parameter that states one: meeting `<+T>` makes it positive, meeting
`<-T>` negative, and standing under a `dual` flips it. So a lambda parameter
used only where `<+T>` is declared needs no annotation, and a type that meets
both a `<+T>` and a `<-T>` is refused — no type is both positive and
negative — before anything solves it.

### Continuation rows

The continuation parameters of a consumer transformer, and the second parameter
group of a `proc`, form that declaration's **continuation row**. A row is
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
go unused — a `proc` that reaches one continuation and ignores the rest is
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
proc pick<E>(c: Bool) | (then: Delayed<(;), ..E> & otherwise: Delayed<(;), ..E>) / {..E} {
    of c { True => then, _ => otherwise }       // runs the exit
}

proc forward<E>(c: Bool) | (then: Delayed<(;), ..E> & otherwise: Delayed<(;), ..E>) / {..E} {
    <c | pick | (then & otherwise)>               // passes them on
}
```

What *is* enforced is that control is **total**: a `proc` body must be `⊥`
— it reaches a continuation on every path — so a body that falls off the end
(a bare value) or dangles (an `of` with an arm that yields a value) is
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
takes no mark: `func map<+A, +B, E>`.

The mark is held against every use. A call, a function named as a value, and
a construction — a variant, a record, a `mu` over a generic menu — give each
parameter a type once the declaration's unification has finished, and a
positive parameter given a function, a consumer or a menu is refused, as is
a negative one given data. Inside a generic body a parameter carries its own
mark, so `func f<-U>(x: U) -> U { <x | id }` is refused when `id` declares
`<+T>`. A type written in a declaration's signature is held to the same rule:
with `enum List<+T>`, `List<-i64>` and `List<(i64 -> i64)>` are refused, and
a list of consumers is a declaration of its own.

An unrestricted parameter can forward or store its argument, but cannot
assume it is positive or negative. `func id<*T>(value: T) -> T { value }`
accepts both data and functions. Passing its `T` to a `<+U>`-only function
is refused. A computation whose result has unrestricted polarity needs
an explicit evaluation choice if inference cannot settle that polarity.

The mark on a generic parameter fixes the polarity of its instantiation.
With `<+T>`, `T` is positive; a bare `T` in a continuation row denotes its
dual consumer, not a negative instantiation of `T`:

```sl
func id<+T>(value: T) -> T { value }
func consume<+T>(ok: T) <- T { ok }
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
is refused with a pointer at `of`.

```sl
let (a, b) = pair;
let Point { x, y } = origin;
func skew((a, b): (i64, i64), c: i64) -> i64 { <(a, c) | mul | x => (x, b) | sub }
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

`let … else` is out of scope: a binder that may fail is an `of`.
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
positive type is refused: a value delayed would bring back the `↑` that
[§8](data.md#no-shifts-a-consumer-is-a-value) removed, so `Lazy<T, E>` stays
the explicit menu spelling for a delayed
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
`let+` would; and a value — a literal, a name, a `func`, a `mu`, a
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
that use, so a `let-` written inside a `do` and used after the `do`
has returned answers to the handlers outside it. `let+` is how a computation
performs under the handler where it is written. What a delayed computation
performs rides on its type as a row ([Effects and handlers](effects.md)), so
it follows the computation wherever it goes — bound again, stored in a tuple,
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
let+ ready = do { let+ value = pending; value } {
    build(): resume => <10 | resume
};
```

Merely handling the expression `pending` does not force it, and a `let+`
outside that handler cannot move forcing back inside it.

```sl
let g = do { let- f = make(); f } { throw(m) => fn(n: i64) { 0 } };
<5 | g | println;              // refused: `make` performs `Exn` here, unhandled

let pair = (1, make());        // stored, it performs nothing yet
let r = do <5 | pair.1 { throw(m) => -1 };   // and performs here, handled
```
