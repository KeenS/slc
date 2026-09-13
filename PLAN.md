# Slant: the standing plan

Slant is a Rust-flavoured surface over a classical λ̄μμ̃ core. The redesign
that established it is finished and recorded in `docs/HISTORY.md`; the
language itself is defined in `DESIGN.md`. This file is neither — it holds
only what is still open: the known limits, the work queued next, and what is
deferred for discussion.

The contract that keeps it short: a decision, once made, goes to `DESIGN.md`
and leaves this file. The plan is where work stops being open, not where it
is remembered — so an entry here is a promise still outstanding, and nothing
else belongs. Entries refer to each other by name, never by position.

## Status

The language is complete and the acceptance suite is green. Traits (ad-hoc
polymorphism) and algebraic effects with handlers are both shipped and
specified in `DESIGN.md`, and the execution model has been rebuilt around
them: the evaluator is an abstract machine over a closed, flat, de-Bruijn
instruction stream, its continuation first-class data (`DESIGN.md` §11). So
effect handlers are multi-shot, captured continuations are cheap and
reusable, and trait dispatch is resolved entirely at compile time.

No large feature is mid-flight. The open work is a set of surface
simplifications that remove syntax in favour of ordinary declarations, a
documentation pass, and one defect.

## Known limits

### Of the design

- **⅋ commutes only where one value meets one declared type.** `A ⅋ B`
  and `B ⅋ A` are one type, and a value is accepted at either spelling where
  it is cut into a consumer, stored in a record field, a variant or a
  `let`, passed as a written argument, or returned — the checker records a
  swap and lowering turns the closure around. Inside a type constructor —
  a tuple's component, `List<A ⅋ B>` against `List<B ⅋ A>` — there is no
  one value to turn, so the spelling still has to match; nor is an argument
  turned when it is the result of an earlier stage rather than a written
  value. Both would need the swap mapped through a structure or a chain.

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
  inference) is the known upgrade if these bite. A stage's row variables
  are instantiated only for the first stage of a chain, since that is the
  only one whose argument is syntax.

- **The file operations perform `IO` without an operation.** The `fs`
  module's `read`, `write`, `open`, `read_line`, `close` and `exists`, and
  the `__` primitives beneath them, charge `{IO}`, so their rows are honest,
  but they reach the outside world directly rather than by performing an
  operation the way `println` does — so they cannot be mocked by a handler.
  The outcome is not the obstacle; a reusable handler is. See "File
  operations as their own effect, and handlers as values". `DESIGN.md`'s
  `IO` section still gives the old reason, that an outcome needs a type the
  operation can name.

### Defects

- **Exhaustiveness does not know `bool`.** A `match` with `true` and `false`
  arms is reported non-exhaustive unless it adds a `_`. The fix is
  "`bool` is defined in the prelude".

## Next

### Documentation

- **`DESIGN.md` is cleaned up.** Parts of it predate the surface changes it
  documents, and its `sl` blocks are not checked, so nothing catches the
  drift. What a pass has to cover:

  - **Connective glyphs.** "Connective spellings" keeps `⊗`, `⅋` and `⊥`
    for the core's terms and the prose, never for a program. The `sl` blocks
    break that in comments (`// -Shape ⅋ +i64`, `// ⅋ every field, wanted`,
    ``// dual(Reading) is `-i64 ⅋ -String` ``), and the prose names surface
    types by glyph throughout ("`⅋` is commutative", "a `⅋` value", a form
    denoting `-i64 ⅋ -String`). Decide whether prose about a surface type
    spells it `;`, a joint, and keeps the glyphs for core terms only. Then
    apply the rule everywhere, this file included.
  - **Chains without `⟨`,** which the surface now refuses:
    `(2, 40) | total | out⟩`, `{ label | println; value | out⟩ }`,
    `"s" | f | str_len | println`, and §13's own entries.
  - **Calls in the refused `f(a)` form,** in code and prose:
    `select { n => println(n) }`, `map(half, xs)`, `drop(42)` (said to
    perform the operation), `handle(k)`, `take(s, n)`, `inner::deep()`,
    `lem()`.
  - **Examples that do not check.** The modules example's `main` prints
    without `/ {IO}`, and the handlers example answers `flip()`, which
    neither `Exn` nor `Reader` declares.
  - **§13, the migration summary,** is a change log inside the definition.
    It belongs in `docs/MIGRATION.md` or `docs/HISTORY.md`, and its entries
    use the old forms too.

  Keeping it clean afterwards means checking the complete programs in
  `DESIGN.md`, for instance by extracting them into the examples test.

### Surface simplifications

Each entry removes a piece of syntax in favour of an ordinary declaration.
They depend on each other, so they are listed in the order they can land:
guards go before `if` can stop being a keyword, chains have to stop nesting
before operators become stages, and `bool` becomes a declaration last, once
nothing left in the language is built on the built-in one.

- **Match guards are removed.** `p if e => …` goes from `match`, and a
  test that needs one is a `match` inside the arm, as §7 already says of
  `select`. Nothing written in Slant uses a guard: no stdlib module,
  example or doc. Only the Rust tests do (`roundtrip.rs` and unit tests in
  the parser, lowering, checker and exhaustiveness). What goes with it:

  - the parser's guard and `MatchArm::guard`, and the resolver's passes
    over it;
  - the checker's `+bool` check on a guard, and the "unguarded" reasoning in
    exhaustiveness, whose diagnostic says "add an unguarded `_` arm";
  - the guard slot of `__match_arm` and the runtime's `MatchGuard` frame.
    `__match_dispatch` itself stays, since literals, or-patterns and a
    default among labelled arms are still order-sensitive;
  - guards in `DESIGN.md`: the `expr.match` lowering row, and §7's reason
    `select` has none, which then only needs to speak of literal arms.

- **`if` becomes a prelude command.** The `if` expression is removed in
  favour of an ordinary declaration in the prelude:

  ```sl
  command if(c: bool) | (then: -(,) & otherwise: -(,)) {
      match c { true => ⟨(,) | then⟩, false => ⟨(,) | otherwise⟩ }
  }
  ```

  It has to be a `command`, not a `fn`: a function's arguments are
  evaluated before the call, so an `if` taking its branches as values would
  run both, and every recursion an `if` guards would diverge. A row delays
  both, and the `bool` picks one — `⟨c | if | (fn(_) { … } & fn(_) { … })⟩`,
  under a `mu` where the `if` is to produce a value. The shape works today
  under another name. The `fn(_)` wrappers exist only to delay the branches;
  "Negative positions by name" would let a bare block stand in their place.

  What stands between it and the name `if`:

  - `if` is a keyword. Once "Match guards are removed" lands, nothing else
    spells it, so it simply stops being one.
  - The prelude body needs a `_` arm until "`bool` is defined in the
    prelude" lands.
  - `&&` and `||` expand to the `if` expression to short-circuit. They
    become functions in "Infix operators become functions", and until then
    they need to lower to a `match` on the `bool`. `__if_dispatch` and the
    `expr.if` lowering row then go.
  - The rule that a `⊥` branch constrains nothing (§3), and the note that
    an `else`-less `if` dangles, leave `DESIGN.md` — each continuation of a
    row is typed on its own, so there is no join to state.
  - Every use migrates: the stdlib, the examples, the docs, and the Rust
    tests' sources. A value-returning `if` grows a `mu`, and an `else if`
    chain becomes nested bundles, which is the real cost to weigh first.

- **A chain through a stage with more than one argument stops nesting.** A
  stage supplies the whole group (§3), so a stage that takes anything
  besides what flows in has to have the chain so far wrapped in parentheses
  and packed into a tuple with the rest. Each such stage adds a level, the
  data ends up in the middle of the expression, and it reads inside out.
  `DESIGN.md`'s own `Seq` example shows it:

  ```sl
  ⟨(⟨(odd, 1 | stream::count_from | seq::of_stream) | seq::filter, 4) | seq::take
  ```

  The same program should read as one flat chain from left to right, with
  each stage's other arguments written at that stage. Whatever syntax does
  this has to fit with `f(a)` being refused and with a stage taking its
  whole group, since both exist so that a call and a chain are not two
  things to learn. It also has to say where in the group the flowing value
  goes: `seq::filter` takes it last and `seq::take` takes it first.

- **Infix operators become functions.** `+ - * / % == != < > <= >= && ||`
  leave the surface, and each is an ordinary function a value flows into:
  `⟨(a, b) | add`. Most already are one underneath. The operators lower to
  the builtins `add`, `sub`, `mul`, `div`, `rem`, `eq`, `ne`, `lt`, `gt`,
  `le` and `ge`, and `⟨(1, 2) | add` and `⟨(1, 2) | lt` run today. What the
  operators do that the functions do not yet:

  - **Overloading.** The checker types an operator by hand: arithmetic at
    any integer width, with a literal adapting to the other side; `+` on
    `String`; comparison on numbers, `char`, `String` and `bool`. The
    builtins' signatures are `(i64, i64) -> i64` and the like, and a stage is
    held to them, so `⟨("a", "b") | add` is refused. As functions they need
    traits, the way `Display` works, or one name per type.
  - **Short-circuiting.** `&&` and `||` expand to the `if` expression so
    their right operand runs only when needed. As functions (`and` and `or`
    are free names) the right operand has to arrive delayed. It is a `bool`,
    a positive type, so "Negative positions by name" does not reach it. What
    works today is `Lazy<bool>`:

    ```sl
    fn and(a: bool, b: Lazy<bool>) -> bool {
        match a { true => b.force, _ => false }
    }
    ```

    This short-circuits: `⟨(false, rhs) | and` never forces a right operand
    that divides by zero, and `⟨(true, rhs) | and` does. The cost is a
    `mu Lazy { force <= ⟨e | force⟩ }` at every call. Delaying a positive
    operand without writing it out is the `↑` that §8 removed.
  - **Nesting.** `a + b * c` becomes `⟨(a, ⟨(b, c) | mul) | add`, so every
    compound expression runs into the chain-nesting problem, which has to be
    solved first or together with this.
  - **Precedence.** §3's rule that `|` binds more loosely than every
    operator, and operator precedence itself, have nothing left to order.

  Prefix `!` goes too, in "`!` becomes a function". Prefix `-` and postfix
  `a[i]` and `a[i..j]` are not infix either, and whether they go is open. Migration touches every arithmetic and
  comparison in the stdlib, the examples, the docs and the Rust tests.

- **`!` becomes a function.** Logical not leaves the surface as the infix
  operators do, and `not`, a free name, takes its place in the prelude:

  ```sl
  fn not(b: bool) -> bool {
      match b { true => false, _ => true }
  }
  ```

  This runs today: `⟨false | not` is `true`. It needs no delayed operand,
  having only one, and it needs the `_` arm until "`bool` is defined in the
  prelude" lands. What goes: the `Bang` token (`!=` is lexed apart and goes
  with the infix operators), `UnOp::Not` in the parser and the checker, its
  lowering to `eq(a)(false)`, and `!a` in `DESIGN.md`'s `expr.unop` row. The
  one use in Slant is `examples/comparison.sl`'s `true && !false`.

- **`bool` is defined in the prelude.** The last of these, and possible
  only once "`if` becomes a prelude command", "Infix operators become
  functions" and "`!` becomes a function" have removed the `if` expression,
  `&&`, `||` and `!`. All of them are built on the built-in `bool`:
  `__if_dispatch` tests a `Value::Bool`, the logical operators expand to
  `if`, and `!a` lowers to `eq(a)(false)`. `bool` then becomes an ordinary
  `enum`, so a `match` on it is exhaustive the way a match on any enum is,
  and the `bool` exhaustiveness defect goes with it: coverage is counted
  only over enum variants (`exhaustive.rs`), and a match over literals
  always asks for `_`. A declared `enum Boolean { False, True }` matched on
  both variants is accepted and runs today. What the change touches:

  - **The spelling.** `enum bool { false, true }` does not parse, because
    `true` and `false` are keywords. Keeping the lowercase names means the
    keywords go and a variant may be spelled lowercase; the other way is a
    Rust-unlike `Bool { False, True }`.
  - **The built-in `bool` everywhere else:** the `true`/`false` tokens,
    `Expr::Bool` and `Pattern::Bool` in the front end, the core's
    `Base::Bool` (types, typing, printing and the core tests), and the
    runtime's `Value::Bool`. The builtins that produce one — the
    comparisons, `is_digit`, `is_ws` and `__file_exists` — have to produce
    the variant instead, and diagnostics that say "expected +bool" name the
    declared type.
  - **The `_` arms added meanwhile.** The prelude's `if`, `and`, `or` and
    `not` match on `bool` with a `_` arm until this lands; they can then
    name both variants.

## Deferred, for discussion

- **Negative positions by name.** The critical pair `⟨μα.c ∥ μ̃x.c'⟩` is
  the choice between by-value and by-name, and it is settled for the
  producer at every type today (`reduce.rs`). Settled by polarity instead, a
  `⊥` item of a row would be a thunk, run by naming it, and `if` would need
  no wrappers:

  ```sl
  command if(c: bool) | (then: (;) & otherwise: (;)) {
      match c { true => then, false => otherwise }
  }

  mu i64 { r <= ⟨n > 0 | if | ({ ⟨n | r⟩ } & { ⟨0 - n | r⟩ })⟩ }
  ```

  Today the checker refuses this bundle, because an item that ends in a cut
  would jump while the bundle is built (`DESIGN.md` §4); by name, each item
  would be a thunk and the refusal would lift. Only negative positions would
  change: delaying a positive
  value without writing it would bring back the `↑` that §8 removed, so a
  value-returning `if` keeps its `mu`, `Lazy<T>` stays the spelling of a
  delayed value, and a `bool` operand of `&&` is not reached. A by-name
  continuation named twice runs twice, which is consistent with
  continuations already being multi-shot.

- **File operations as their own effect, and handlers as values.** The
  `fs` module would declare an `Fs` effect, and a standard handler would
  answer it by performing `IO`. A program that touches files would then say
  `{Fs}` in its row, and a test could mock the file system with a handler of
  its own, the way `examples/io.sl` mocks output.

  The effect itself works today. An operation can take its outcome
  continuations as parameters, and a clause below its own prompt performs
  `IO` outward. This reads a file, and routes both outcomes, under an inline
  handler:

  ```sl
  effect Fs { fn read_file(path: String, ok: -String, failed: -String) -> (;); }

  handle mu String { k <= ⟨("input.txt", k, select String { … }) | read_file } {
      read_file(path, ok, failed) => ⟨path | __read_file | (ok & failed)⟩,
      return(s) => s,
  }
  ```

  What does not work is offering that handler for reuse. `handle` installs
  clauses only where it is written, so `fs` has nothing to export as "the
  real file system". A function that installs the handler around a lambda,
  `fn with_real_fs<T, E>(body: ((,) -> T / {Fs, ..E})) -> T / {IO, ..E}`, is
  refused: the lambda's `Fs` is charged to the declaration that wrote it
  ("Effect tracking follows names"), so the caller is told it performs
  `Fs`. Eff answers this with first-class handlers — `handler { … }` is a
  value, and `with h handle c` installs it. What to discuss:

  - **The type of a handler value.** It has to say which effect it
    discharges, what its clauses perform in turn (`{Fs}` to `{IO}`), and how
    its return clause maps the body's result. That probably wants rows in
    types first, the upgrade "Effect tracking follows names" already names.
  - **Its place in the polarity story.** A handler consumes a computation,
    and its clauses bind continuations the copattern way, so it may be a
    negative value like a menu or a form rather than something new.
  - **Who installs the standard `Fs` handler.** `main` may leave only `IO`
    undischarged. So either the runtime installs this one too, making `Fs`
    a second special effect, or the driver wraps `main` in the prelude's
    handler, or every program installs it.

- **Replacing `⟨` and `⟩`.** The cut brackets are the last non-ASCII
  surface syntax; their replacement is to be designed.
