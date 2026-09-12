# Slant: the standing plan

Slant is a Rust-flavoured surface over a classical λ̄μμ̃ core. The redesign
that established it is finished and recorded in `docs/HISTORY.md`; the
language itself is defined in `DESIGN.md`. This file is neither — it holds
only what is still open: the known limits, the deliberate deferrals, and the
work queued next.

The contract that keeps it short: a decision, once made, goes to `DESIGN.md`
and leaves this file. The plan is where work stops being open, not where it
is remembered — so an entry here is a promise still outstanding, and nothing
else belongs.

## Status

The language is complete and the acceptance suite is green. Traits (ad-hoc
polymorphism) and algebraic effects with handlers are both shipped and
specified in `DESIGN.md`, and the execution model has been rebuilt around
them: the evaluator is an abstract machine over a closed, flat, de-Bruijn
instruction stream, its continuation first-class data (`DESIGN.md` §11). So
effect handlers are multi-shot, captured continuations are cheap and
reusable, and trait dispatch is resolved entirely at compile time. No large
feature is mid-flight; what remains open is below.

## Known limits

- **No partial application.** `f(a)` of a two-parameter `f` does not
  produce a function awaiting the second argument: the call's type is the
  full result, so `7 @ route("high")` is refused even though
  `route(tag, x) -> ⊥` would make the partial application a consumer of
  `+i64`. Calls are curried in the core and the runtime accumulates
  arguments, so this is a checker-side gap — the type of an
  under-applied call — not a representational one. (Found while
  establishing that operations need no negative form.)

- **A file handle's close is not enforced.** `+File` is the first resource
  with a lifetime, and nothing checks it: an unclosed handle leaks until the
  program ends, and only a read after `close_file` fails. Enforcing it would
  take a dedicated resource/ownership check (the value side of the language is
  otherwise unrestricted — see `DESIGN.md` §4). Until then the idiom is
  composition at the door: shadow `exit` with
  `select +i32 { status => { close_file(handle); status @ exit } }` where the
  handle comes into scope, and no later path can leave the file open —
  `examples/file_io.sl` does exactly this.

- **Soundness is enforced by inference, argued informally.** What remains
  short of a proof: no mechanized subject-reduction argument ties the checker
  to the reduction rules, comparing two values nothing else constrains stays
  unchecked, type variables carry no polarity kind, and the untyped evaluator
  remains the backstop for whatever that gap hides.

- **Effect tracking follows names.** Rows and row variables are explicit
  and checked per declaration, but the rows live beside the type system
  rather than in core types: a lambda's effects are charged where it is
  written, a higher-order global passed as a value forwards nothing
  further, and a function laundered through a `let` binding is not
  tracked. Moving rows into the arrow type itself (unified during
  inference) is the known upgrade if these bite.

## Next

- **Flow, rows, and the nullary spellings — the composition redesign.**
  Decided in discussion; the decisions first, then the steps.

  *Decisions.* `|` is **flow**, and it is the only connective: `@` is
  abandoned. `v | f` applies, `f | g` composes functions, `f | k`
  composes a function into a consumer (retiring the prelude's `then`),
  and `v | k` is the cut. A pipeline is named by its hole: `v | f` awaits
  a continuation (it is a value), `f | k` awaits a value (it is a
  consumer), and a pipeline closed at both ends — value at the left,
  consumer at the right — is a command. The pipe reads left to right, so
  the orientation rule survives as: a consumer may stand only at the
  right end, and nothing flows out of one (`v | k | x` is refused, as
  chaining a cut is today). Calls and rows become **unary**: `f(a, b)`
  is `f((a, b))`, and a continuation row is one parameter — a **negative
  additive**, since the caller supplies every exit and the callee takes
  exactly one, and `dual(-A & -B) = A ⊕ B` says a two-exit command yields
  one of two outcomes. A row is therefore an *anonymous menu of
  consumers*, and `| (found, missing)` is its copattern, as `(x, y)` is a
  tuple's. **Headers omit signs where the position decides them**: the
  value group `(v1: T1, v2: T2)` is `T1 ⊗ T2`, and the continuation group
  `(k1: T1 & k2: T2)` names what *reaches* each exit — its type is
  `-T1 & -T2`, the sign implied by the group, the way a `menu` lists what
  each item answers. So `command c(v1: T1, v2: T2) | (k1: T3 & k2: T4)`
  has type `(T1 ⊗ T2) → (T3 ⊕ T4)`, and a negative function
  `fn f(k1: T1 & k2: T2) <- T3` — no value group, only the bundle — is
  `T3 → (T1 ⊕ T2)` by involution: it takes a `T3` and delivers to one
  exit. A one-exit command is then the positive function's type exactly,
  which is the two-styles story already told. Written as a standalone
  *type expression*, `&` stays the plain connective and a bundle of
  exits is `(-A & -B)` with its signs; only header groups imply them.
  Spellings: `&` is lexed; the anonymous type is `(A & B)`; the
  bundle literal is `(k1 & … & kn)` for two or more exits (a `mu` cannot
  forward an existing continuation as an item — the arm would put a
  consumer on the value side of a cut); one exit is the continuation
  itself (`-A & ⊤ ≅ -A`); and *a paren holding only the separator is the
  nullary form*: `(&)` is ⊤'s unique value (the empty menu — the prelude's
  `menu Top {}`, not Bottom, which is the nullary form), `(,)` is unit,
  replacing `()` as a value; `f()` remains the zero-argument call, which
  also retires the runtime's separate no-arguments marker.

  - [ ] Lex `&`; parse `(A & B)` as `TypeExpr::With` lowering to
        `Type::With`, and `(&)` in type position as the prelude's `Top`.
  - [ ] Parse `(k1 & k2 & …)` to a bundle expression, `(&)` to ⊤'s value,
        `(,)` to unit; remove `()` as a value expression (keep `f()`), and
        drop `Value::NoArguments` in favour of unit. Migrate every `()`.
  - [ ] Type the bundle as `With` of its components' types; project it
        positionally (`out.0`, `out.1`) through `Expr::Project`, so
        `n @ out.0` takes an exit. Runtime: a bundle is a pair chain, as a
        tuple is — the machine is untyped.
  - [ ] Retype rows: a command's continuation group `(k1: T1 & k2: T2)`
        is one parameter of type `-T1 & -T2` — the group's `&` separates
        components and implies their sign; a negative fn's group is the
        same, and the polarity rule accepts a `With` of consumers as a
        continuation parameter. The copattern `| (a & b)` binds the
        components. Call sites pass a bundle; the positional per-slot
        check becomes one type check. Headers: `(v: T)` in a value group
        and `(k: T)` in a continuation group both omit the sign; a sign
        stays legal and must agree. DESIGN's "row" wording moves from
        positional to `&`, and its polarity section states the rule that
        signs are written only where position does not decide.
  - [ ] Unary values: `f(a, b)` packs a tensor and the declaration
        destructures it (`μ̃(x, y)`), replacing curried lowering; function
        values then have single-argument types, and `parse_int`-style
        builtins take their arguments the same way. This is the step
        that makes every stage of a pipeline one-in, one-out.
  - [ ] `|`: a binop at a new lowest level (below `||`), associative.
        Checker: by the operands' polarity — value into function applies,
        function into function composes, function into consumer builds a
        consumer, value into consumer is the cut (type `⊥`); a consumer
        anywhere but the right end is refused. Lowering: application, a
        composed closure, the `then` shape, and today's `Expr::Cut`
        respectively — the cut node stays internally as the closed case.
  - [ ] Retire `@`: remove the token and the cut parse path; migrate
        every `@` in the prelude, examples, tests, DESIGN, and the design
        notes to `|`. The effect checker's cut-site charging (latent rows
        at a feed, a rowed consumer parameter, a result-latent call in
        consumer position) moves to the closed-pipeline case. DESIGN's
        "the left of `@` is the value side" becomes the pipe's
        orientation rule.
  - [ ] Delete `then` from the prelude; rewrite its uses as `f | k`.
  - [ ] Examples: `pipeline.sl` showing the three readings and the
        associativity (`v | f | g @ k` and `v @ f | g | k` agree), a
        two-exit command written unary. MIGRATION: `@` → `|`, `()` → `(,)`,
        rows, `then`. DESIGN: the flow operator and its four readings,
        rows as `&`, the nullary rule.

## Deferred, for discussion

- **A surface spelling for `0`.** ⊤'s value is settled as `(&)` (above).
  `0` has no values, and its consumer stays `select Empty {}`; whether the
  empty sum deserves an anonymous type spelling is still open.

## Deferred, with no accepted replacement

- **Trait objects (`dyn`).** Dispatch is static — a concrete call goes direct,
  a bounded call through a dictionary — with no runtime method value, so there
  is no existential package that hides a value's type behind its trait. `dyn`
  (a value carried together with its dictionary) remains open.

- **The interaction-net backend.** An unwired experiment: `slc-core::net`
  and its bridge were reachable only from their own tests, never from the
  pipeline. Removed as dead code; git history has it, and the abstract
  machine the redesign built is the evaluator now.
