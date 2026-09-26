Part of the [language design](../../DESIGN.md).

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
func area(s: Shape) -> i64                  // (-Shape ; +i64)
func area_of(out: i64) <- Shape             // (+i64 ; -Shape) — adapted orientation
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

func deliver(out: String) <- i64 {
    mu i64 { number => <number | int_to_str | out> }
}

proc main | (exit: i32) / {IO} {
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
its argument by the polarity rule in [§4](polarity.md): positive computations
run before the stage, negative computations wait for demand. The rule includes a
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

**A `proc` is a stage too.** It takes two groups — values, then the
menu of exits — and the chain hands it both: what flows in is the value
group, and the closing stage is the row. So a command reads like every
other call, and ends where control leaves it:

```sl
proc nth<+T>(xs: List<T>, i: i64) | (found: T & missing: String)

<(xs, 2) | nth | (found & missing)>
```

The row travels whole, so a command that takes one may hand it on
unopened — `proc forward(…) | (row: (-T & -String)) { <(xs, 2) | nth | row> }`.
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
and only a stage after `|` can be one: the head of a chain and an `of` arm
keep their meaning.

**A cut** `<v | k>` sends the value `v` to the consumer `k`. It is the surface
spelling of the core's `⟨ v ∥ k ⟩`, and it is a *command*, not an expression
that happens to return: control does not come back, so nothing after it in a
block runs, and its type is `⊥`.

```sl
proc route(x: i32) | (k: i32) {
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
`of c { True => <(pos, 1) | add, False => <message | err> }` the `of` has the type of
the arm that returns, and arms that both return must agree.

Calling a continuation is rejected. `k(v)` reports that `k` is a consumer and
not a function, because a reader — and the compiler — should not have to know
what `k` is bound to in order to tell an application from a command.
