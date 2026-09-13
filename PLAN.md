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

No large feature is mid-flight. The open work is settling evaluation by
polarity, a set of surface simplifications that remove syntax in favour of
ordinary declarations, and one defect.

## Known limits

### Of the design

- **`;` commutes only where one value meets one declared type.** `(A ; B)`
  and `(B ; A)` are one type, and a value is accepted at either spelling where
  it is cut into a consumer, stored in a record field, a variant or a
  `let`, passed as a written argument, or returned — the checker records a
  swap and lowering turns the closure around. Inside a type constructor —
  a tuple's component, `List<(A ; B)>` against `List<(B ; A)>` — there is no
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

- **A `mu` that performs escapes a resuming clause.** A `mu` captures the
  whole continuation, past any handler's prompt. When its body performs an
  operation, a clause that resumes more than once loses every resumption
  after the first: the first jump to the `mu`'s continuation leaves the
  clause instead of returning into it. Under
  `flip(): resume => (⟨true | resume) + " " + (⟨false | resume)`,
  `let a = mu String { r <= ⟨(match flip() { true => "H", _ => "T" }) | r⟩ }`
  answers `"H"`; the same program without the `mu`, or with `flip()`
  performed before it, answers `"H T"`. Stopping a `mu`'s capture at the
  nearest prompt would change what `mu` means under a handler, so this is a
  design question, set aside for now. It is what kept `if` from becoming a
  prelude command: a value-returning one needs a `mu` around a condition
  that may perform, so `if` became a `match` instead.

### Defects

- **Exhaustiveness does not know `bool`.** A `match` with `true` and `false`
  arms is reported non-exhaustive unless it adds a `_`. The fix is
  "`bool` becomes the prelude's `Bool`".

## Next

### Evaluation

Independent of the surface simplifications below: `&&` and `||` are gone
rather than made functions, so no operator waits on how an operand is
delayed.

- **Negative positions by name.** The critical pair `⟨μα.c ∥ μ̃x.c'⟩` is
  the choice between by-value and by-name, and it is settled for the
  producer at every type today (`reduce.rs`). It is to be settled by
  polarity instead: a computation of negative type is not run where it is
  written, but each time it is demanded. Decided:

  - **Every negative position.** An argument, a tuple component, a bundle
    item and a binding all delay a negative computation — a block that ends
    in a cut, or a call that returns a consumer, a function or a menu. A
    positive value is still computed where it is written: delaying one
    would bring back the `↑` that §8 removed, so `Lazy<T>` stays the
    spelling of a delayed value.
  - **Run where demanded.** A delayed value runs where its result is needed
    — as a command, cut into, applied or demanded — and is passed on unrun
    into another by-name position: an argument, a component, a bundle item
    or a `let-`. So a command that picks an exit needs no `fn(_)` wrappers:

    ```sl
    command choose(c: bool) | (then: (;) & otherwise: (;)) {
        match c { true => then, _ => otherwise }       // runs the exit
    }

    command forward(c: bool) | (then: (;) & otherwise: (;)) {
        ⟨c | choose | (then & otherwise)⟩               // passes them on
    }
    ```

  - **Re-run at each use.** Nothing is cached: a delayed value demanded
    twice runs twice, effects included, as continuations are already
    multi-shot. `naturals` in `examples/seq.sl` is rebuilt at each use.
  - **Effects are latent.** Nothing is performed where a delayed computation
    is written: its row moves onto its type, and each use performs it, so
    the handler that must discharge it is the one around the use — the rule
    rowed menus and returned consumers (`-> (-A / {..E})`) already follow.
    Only a concrete row can ride on a type today, so until the rows-in-types
    upgrade that "Effect tracking follows names" names, a computation in a
    by-name position whose row is a variable is refused.
  - **Bindings are done.** `let+`, `let-` and a plain `let` that follows its
    type have landed (`DESIGN.md` §4, "When a `let` computes"): a delayed
    value is `λ$delay. t` in the core and runs wherever it is applied, cut
    into or asked for an item. Arguments, tuple components and bundle items
    are the positions left.
  - **An unknown polarity is an error.** A binding's is refused already; a
    lambda parameter whose type inference leaves a variable is to be refused
    the same way, asking for an annotation.
  - **Printing is already generic over `Display`.** `println` and `print`
    are prelude functions over `<T: Display>`, so no builtin is polymorphic
    over polarity; their parameter takes its `+` with every other.

  What it changes, as found so far:

  - The refusal of a bundle item that ends in a cut (`DESIGN.md` §4) lifts:
    the item is delayed instead. Written by hand as the consumer of unit it
    is, `select unit { u => … }`, such an item already runs only when the
    command sends it `⟨(,) | then⟩`.
  - A bare `then` passes the checker today and does nothing at run time —
    `main` ends holding the consumer — so running where demanded is new work
    in the checker, which knows where a name is demanded, and in lowering.
  - `examples/connectives.sl` flows `mu (;) { k <= … }` into `println` and
    prints `(,)` because it runs at once; it becomes a `let+`, rendered
    through `Display`.
  - Lowering needs each position's polarity from the checker, as `pars`
    already carries a joint's components.

### Surface simplifications

Each entry removes a piece of syntax in favour of an ordinary declaration.
They depend on each other, so they are listed in the order they can land:
`bool` becomes the prelude's `Bool` last, once nothing left in the language
is built on the built-in one.

- **Infix operators become functions.** `+ - * / % == != < > <= >=`,
  prefix `-`, and indexing `a[i]` and `a[i..j]` leave the surface, and each
  is an ordinary function a value flows into: `⟨(a, b) | add`. Most already
  are one underneath — the operators lower to the builtins `add`, `sub`,
  `mul`, `div`, `rem`, `eq`, `ne`, `lt`, `gt`, `le`, `ge` and `neg`, and
  `⟨(1, 2) | add` runs today. Decided, and still to do:

  - **Overloaded by traits.** The checker types an operator by hand today:
    arithmetic at any integer width, `+` on `String`, comparison on numbers,
    `char`, `String` and `bool`. As functions they are trait methods, the
    way `Display` is, with impls for each integer width and for `+` on
    `String`. Traits today have one `Self`, no associated types, and method
    names unique across traits, which these fit.
  - **Literals keep adapting.** `1 + x` with `x: i32` adapts the literal to
    `i32` today. With dispatch on the first argument the literal would
    default to `i64` and fail, so an integer literal has to keep taking its
    width from the other operand.
  - **Indexing is plain functions.** Only a `String` is indexed — by an
    `i64`, giving a `char` — and a trait could not say that in general
    without associated types. `a[i]` and `a[i..j]` become functions over the
    builtins `__index` and `substring` beneath them today.
  - **Compound expressions stay one chain.** `a + b * c` becomes
    `⟨(b, c) | mul | x => (a, x) | add`.
  - **Precedence goes.** §3's rule that `|` binds more loosely than every
    operator has nothing left to order.

  Migration touches roughly two hundred arithmetic and comparison uses in
  the stdlib, the examples, the docs and the Rust tests, so it wants a
  converter, as `if` had.

- **`bool` becomes the prelude's `Bool`.** The last of these. It becomes an
  ordinary enum, `enum Bool { False, True }`,
  so a `match` on it is exhaustive the way a match on any enum is, and the
  `bool` exhaustiveness defect goes with it: coverage is counted only over
  enum variants (`exhaustive.rs`), and a match over literals always asks for
  `_`. A declared `enum Boolean { False, True }` matched on both variants is
  accepted and runs today. What the change touches:

  - **The names.** `true` and `false` stop being keywords, and every `bool`,
    `true` and `false` a program writes becomes `Bool`, `True` and `False`.
  - **The built-in `bool` everywhere else:** `Expr::Bool` and
    `Pattern::Bool` in the front end, the core's `Base::Bool` (types,
    typing, printing and the core tests), and the runtime's `Value::Bool`.
    The builtins that produce one — the comparisons, `is_digit`, `is_ws` and
    `__file_exists` — have to produce the variant instead, and diagnostics
    that say "expected +bool" name the declared type.
  - **The `_` arms added meanwhile.** Every `match` on a `bool` — the
    prelude's `not`, and those the stdlib and examples use where `if` used to
    be — has a `_` arm until this lands; it can then name `False`.

## Deferred, for discussion

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
  `fn with_real_fs<+T, E>(body: ((,) -> T / {Fs, ..E})) -> T / {IO, ..E}`, is
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
