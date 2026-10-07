Part of the [language design](../../DESIGN.md).

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
writes it directly — `mu` denotes a consumer and lowers straight to it,
and a continuation passed as an argument arrives the same way. Its
elimination is application: `⟨ co(e′) ∥ v · e ⟩ → ⟨ v ∥ e′ ⟩` sends the
argument to the underlying co-term. The reification is invisible at
lowering because the value is
already in this form; the type is what they change.

A declared continuation parameter and `μα. c` both bind a continuation
variable, and they are not interchangeable. A declared parameter is an
ordinary `λ` binder — the caller supplies the continuation, since a
continuation is a value like any other, and that is a `proc`'s row.
`μα. c` captures the *ambient* continuation instead, and that is the `mu`
expression.

### Execution

`slc run` keeps one stack segment. A capture copies the frames down through
the outermost prompt into a heap image. A prompt carries an id, and resume
splices the image at the prompt with that id. A copy invoked under a prompt
the image does not hold is an error. On x86-64 Linux, fuel is the ELF's
argument. With no argument the run is bounded by the segment; `--fuel N`
passes `N`, and `N = 0` runs no SLC code. Exhausting the fuel is an error.
On any other host the same command uses the interpreter.

`--interpret` is the chunk machine: one flat stream of nodes, and a
continuation held as persistent `Rc` conses. `--fuel N` caps that run at
`N` machine steps.

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

### Subject reduction

A command is a term cut against a co-term of the same type: the term proves
it and the co-term refutes it. Two distinct inference variables are not that
one type, and `dual(?i)` is not `?j`, so a cut whose sides nothing else has
solved is not a command. The proof is
[`proof/Slc/Core.lean`](../../proof/Slc/Core.lean), checked with `lake build`
in `proof/`. `step` returns a typed reduct — that is preservation — and
`progress` says a command with no free variable is one of these redexes.
`Ty.dual_dual`, `dual_var_ne_var`, and `distinct_vars_are_not_a_cut` are the
type lemmas. The surface checker refuses the same unsolved cut: a `proc`
body that is still a variable, a `mu` scrutinee nothing has signed positive,
a consumer whose polarity is still unknown, and a pin whose sides are still
flexible variables.

The labelled rule is what makes `mu` lazy: the label of the value selects
one branch, and the branches that were not selected are discarded unreduced.
The copattern rule is its mirror: the request selects one branch of the menu
and binds the continuation it carries. The co-labelled rule is what lets
`of` take a continuation apart — a reified request is a labelled
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
| `expr.let` | `let x = v; e` | `μlet. ⟨ ⟦v⟧ ∥ μ̃x. ⟨ ⟦e⟧ ∥ let ⟩ ⟩` — a binder is `μ̃`, the value abstraction. A binder that is a pattern is the one-arm `of` it abbreviates: `μ__match. ⟨ ⟦v⟧ ∥ μ̃p. ⟨⟦e⟧ ∥ __match⟩ ⟩`, over the same branch table `expr.of` builds. A parameter pattern binds the group to one name and destructures it the same way |
| `expr.block` | `{ e₁; e₂ }` | `μ__seqᵢ. ⟨ ⟦e₁⟧ ∥ μ̃__discarded. ⟨ ⟦e₂⟧ ∥ __seqᵢ ⟩ ⟩` — both generated binders are fresh against the expressions they enclose |
| `expr.flow` | `<v \| k>`, and every other chain | `μ__cut. ⟨ ⟦v⟧ ∥ k ⟩` for a named consumer; for a computed one, first bind the by-name input to a fresh `saved`, then evaluate the consumer and apply it to `saved`. The cut has no result. Open chains fold grouped applications; composition wraps that fold in a λ. A command takes its value group and exit bundle together; yielding exits compose returning callbacks into a fresh captured result continuation ([§5](control.md#5-proc-consumer-abstraction)) |
| `expr.mu` | `mu A { k <= e }` | `μk. ⟨ ⟦e⟧ ∥ k ⟩` — the captured continuation, not a declared parameter; the type in front is what the expression produces |
| `expr.match` | `of s { p => e, … }` | a match the core can express — every arm a shape (variant, record, tuple, request, or one whole-value binder), components binders or nested products, no duplicates — is a genuine cut: `μ__match. ⟨ ⟦s⟧ ∥ μ̃[T; L(x…). ⟨⟦e⟧ ∥ __match⟩ \| … ] ⟩` (`μ̃(x…)`/`μ̃x` for a product/atom). Anything order-sensitive — literals, or-patterns, a default among labelled arms — falls back to `__match_dispatch(⟦s⟧, arm₁, …)`, each arm `__match_arm(descriptor ⊗ λ__match_arg. ⟦e⟧)` |
| `expr.data` | `S { f: v, g: w }` | `S((⟦v⟧ ⊗ ⟦w⟧))` — the declaration's name labelling the tuple of its fields, the same shape a variant has |
| `expr.select` | `mu T { p => c, … }` | `co(μ̃[T; L(x…). ⟦c⟧ … ])` for a labelled type — one branch per shape, the pattern's binders naming that shape's components — and `co(μ̃[T])` when it has no shapes; `co(μ̃(x…). ⟦c⟧)` for a product, and `co(μ̃x. ⟦c⟧)` for an atom, whose one binder takes the whole value |
| `expr.comatch` | `mu T { item: k <= c, … }` | `μ[T; .T::item(k). ⟦c⟧ | …]` — the copattern form of `mu`: a menu value, one branch per demand. Nested copatterns group by their outer destructor: the branch binds `__k`, and its body cuts the inner menu against it |
| `expr.request` | `.item(k)` | `co(.M::item(k))` for a named continuation; any other expression is bound first, then named. A demand `cfg.item` is `μ__ask. ⟨ ⟦cfg⟧ ∥ .M::item(__ask) ⟩` |
| `decl.menu` | `menu M { item: A, … }` | no term of its own: `mu M { … }` builds the `μ[…]`, and its items name the `.M::item(e)` requests. `item(p: P): A` is the item whose answer is `(P -> A)` |
| `decl.form` | `form F { field: A, … }` | no term of its own: `mu F` builds `co(μ̃[F; F(x…). ⟦c⟧])`, and `F { … }` builds the demand `F(⟦v⟧ ⊗ …)` it consumes |
| `expr.consumer_argument` | `<k \| f` — a consumer as an argument | `⟦k⟧` — a consumer is a value; nothing to coerce |
| `expr.handler` | `hn E { clauses }` | a labelled clause tree containing operation closures and the return closure, defaulting to identity; unmatched operations forward outward |
| `expr.with_handler` | `do body h` | runtime handler installation with `⟦h⟧` and a thunk of `body`; inline `do body hn { clauses }` builds the same clause tree |
| `decl.fn.returning` | `func f(x: +A) -> B { e }` | `λx. ⟦e⟧` |
| `decl.fn.transformer` | `func f(k: -A) <- B { e }` | `λk. ⟦e⟧` |
| `decl.mu` | `proc f(x: +A) \| (k: -B) { e }` | `λx. λk. ⟦e⟧` |
| `decl.const` | `def C: +A = v;` | `⟦v⟧` |
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
| `x`, `λx. t` | identifiers, functions, lambdas, and declared continuation parameters (`func … <- …`, a `proc`’s row) |
| `μα. c` | local `mu` expression, a flow that closes against a named consumer, and the lowering of `let`, blocks, and applications |
| `t ⊗ t` | tuple literals, `data` literals, `(A, B)` values |
| `L(t)` | `enum` values and `data` values — a labelled product — and choices `::i(v)`, labelled by their position `\|i` |
| `μ[M; .d(α). c \| …]` | `mu` over a `menu` — the copattern form |
| `.d(e)` | a demand `cfg.item`, and the consumer inside a request literal `.item(k)` |
| `co(e)` | `mu`, and every consumer in value position — a reified co-term, and a `form` value, `(k1 ; k2)` included |
| `α` | the consumer named on the right of a cut, `<v \| k>` |
| `v · e` | application, `<a \| f`, and a cut whose consumer is computed rather than named, which applies the resulting consumer after binding the input |
| `μ̃x. c` | every binder: `let`, a discarded block expression; written directly as `mu +A { x => c }` |
| `μ̃[T; …]`, `μ̃[T]` | `mu` over an `enum`, a `data` or a sum `(A \| B)`, including `mu (\|) {}` |
| `μ̃(x…)` | `mu` over a bare product |
| `prj:i` | `base.i` (tuple) and `base.field` (a record), the field resolved to its index from the base type |

### Classical control

The core is classical, so the classical laws are ordinary programs. Negation
is a consumer — `¬A` is `-A`, since `A → ⊥` and `-A` are one type. Double
negation is an involution, so its elimination is the identity. Excluded
middle uses `mu`, which hands out the continuation of its expression:

```sl
func dne<+T>(value: -(-T)) -> T { value }

// A ⊕ ¬A: answer with the refutation, which is the continuation in disguise.
func lem() -> Choice {
    mu { k <=
        <Choice::Refutes(mu i64 { a => <Choice::Holds(a) | k> }) | k>
    }
}
```

`examples/duality/classical.sl` runs both. The types above go through the
shifts of [§8](data.md#no-shifts-a-consumer-is-a-value) — `-(-i64)` *is*
`+i64`: `dne` is the identity, and `<42 | dne` is `42`.
Involution does not make an integer executable: putting a positive atom on
the consumer side of a cut is rejected after inference, whether named or
computed.

A captured continuation is a value with no expiry: the evaluator is an
abstract machine whose continuation is an explicit frame stack, and `mu`
captures by reifying it. Activating `k` *reinstates* that stack — after the
`mu` has answered, from however deep, as many times as it is reached, down
to the nearest handler the two stacks share
([§6](control.md#6-mu-capturing-the-current-continuation)) — so
taking `lem()`'s offer re-enters the very `of` that already received
`Refutes`, which this time holds.

## 12. Error continuations

Fallible operations receive their result continuations directly. For example,
a parse operation receives both a success continuation and an error
continuation:

```sl
let parsed = mu +String { value => { <("parsed: ", value) | add | println; <0 | exit> } };
let failed = mu +String { message => { <("error: ", message) | add | println; <1 | exit> } };
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
