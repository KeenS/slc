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

- **A file handle's close is not enforced.** `+File` is the first resource
  with a lifetime, and nothing checks it: an unclosed handle leaks until the
  program ends, and only a read after `close_file` fails. Enforcing it would
  take a dedicated resource/ownership check (the value side of the language is
  otherwise unrestricted — see `DESIGN.md` §4). Until then the idiom is
  composition at the door: shadow `exit` with
  `select +i32 { status => { handle | close_file; status | exit⟩ } }` where the
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
  inference) is the known upgrade if these bite. A stage's row variables
  are instantiated only for the first stage of a chain, since that is the
  only one whose argument is syntax.

- **The file builtins perform `IO` without an operation.** `read_file`,
  `write_file`, `open_file`, `read_line`, `close_file`, and `file_exists`
  charge `{IO}`, so their rows are honest, but they reach the outside
  world directly rather than by performing an operation the way `println`
  does — so they cannot be mocked by a handler. Each offers its outcome to
  continuations, and an operation carrying an outcome needs a type the
  operation can name (a generic `IoOutcome<T>`, or one operation per
  outcome shape). Doing it needs a resumption point that dispatches on the
  outcome, which is a frame the machine does not have yet.

## Next

- **A standard library beside the prelude.** Decided: the prelude keeps
  only what every program needs unasked — `effect IO`, `Display` with
  `fmt`/`to_string` and its `i64`/`String`/`bool` impls — and everything
  else moves to stdlib modules a program must `use`: `List` and its
  functions, `Option`, `Result`, `min`/`max`/`abs`, `Stream`, `Seq`,
  `Lazy`, `traced`. Visibility is done (`pub`, private by default). The
  language already reaches a module declared in the library unit from user
  code, qualified or imported, so what remains is plumbing, two real gaps,
  and the move itself.

  *Several source units.*
  - [ ] The driver appends a list of `(unit name, text)` — `prelude.sl`
        then `stdlib/*.sl` — and records each boundary, instead of one
        `PRELUDE` and one `prelude_from`. Spans are char offsets, so the
        boundaries are too.
  - [ ] Each stdlib file declares its module itself — `mod list { … }` at
        the top of `stdlib/list.sl` — rather than the driver wrapping the
        text, so no span shifts and the file reads as what it is.
  - [ ] `resolve_program_split(program, prelude_from)` takes the boundary
        list. `apply_variant_imports` keeps one table per unit
        (`tables: [HashMap; 2]` becomes a `Vec`), so a stdlib file's
        `use Enum::*` is scoped to that file.
  - [ ] **The root scope is shared across units** (`collect_scope` runs
        once over the combined declarations), so a top-level `use a::b;`
        in a library unit would alias `b` in the program's root too. Either
        the root scope's `aliases` become per-unit like the variant tables,
        or a library unit may `use` only inside its `mod` — the second is
        simpler and is how the files would be written anyway; enforce it.
  - [ ] A program's `mod list` colliding with the stdlib's `mod list`:
        decide (error, or the program's shadows) and test it. The prelude
        already has a shadowing rule for top-level names; modules need the
        same statement.

  *Diagnostics name the unit.*
  - [ ] `format_span` maps a span to `(unit, line, column)` from the
        boundary list and prefixes the unit's name, so an error inside the
        library no longer reports a line past the end of the program's file.
        (Found the hard way: prelude errors read as example errors for
        several minutes.)
  - [ ] The resolver's errors, the checker's, and the runtime's `at …`
        all go through the same mapping.

  *`use` for a library.*
  - [ ] `use list;` — importing a module itself — is a parse error ("at
        least two segments"). Allow it, so `use list;` then `list::map`.
  - [ ] `use list::*;` for a *module* — bring every `pub` member in — does
        not exist; `Glob` is variant import only. Decide whether to add it
        or keep imports explicit; the examples will say which reads better.
  - [ ] Check a three-segment variant import, `use list::List::*;`, works
        as written, since that is how every list-using program will start.

  *The move.*
  - [ ] Split `prelude.sl` into `prelude.sl` (logical units, `IO`,
        `Display` and its scalar impls) and `stdlib/{list,option,result,
        num,stream,seq,lazy,trace}.sl`, each a `pub`-marked `mod`.
  - [ ] `impl<T: Display> Display for List<T>` moves with `List`: an impl
        in a module for a trait outside it. Check the mangled impl name and
        `dict_global_name` survive a `::` in the type key (variants already
        carry `::`, so likely fine — verify).
  - [ ] The logical units `Unit`, `Bottom`, `Empty`, `Top` **stay
        top-level in the prelude**: `declarations.rs` recognises `data Unit
        {}` and `form Bottom {}` as the multiplicative units by exact bare
        name, so inside a module they would silently stop being aliases.
        Either keep them where they are, or make the recognition
        path-aware; the first is right unless something needs the second.
  - [ ] The parser's `menu_items` table (for the one-arm `mu M { … }`
        shorthand) is built from bare names before resolution — check `mu
        Stream { … }` still parses when `Stream` is `stream::Stream`
        reached through a `use`.
  - [ ] `traced` performs `IO`; a stdlib module that prints declares
        `/{IO}` — already true, keep it true.

  *Corpus and docs.*
  - [ ] Every example using `List`, `Cons`/`Nil`, `Option`, `Result`,
        `min`/`max`/`abs`, `Stream`, `Seq`, `Lazy` or `traced` gains the
        `use` it needs; outputs are unchanged, so `examples.rs`
        expectations should not move.
  - [ ] The Rust test snippets likewise (`integration.rs`, and the
        check-crate tests that spell out `List::Cons`).
  - [ ] `the_prelude_is_available_and_shadowable` in `integration.rs`
        becomes two tests: the prelude is available unasked; a stdlib
        module is not until `use`d.
  - [ ] DESIGN §"Standard library" describes the two layers and what each
        holds; §10 gets the per-unit scoping rule; MIGRATION shows the
        `use` a program now writes.
  - [ ] An example that uses a stdlib module and nothing else, so the
        boundary is visible in the corpus.

  *Accepted for now.* Every library unit is parsed and checked on every
  run, `use`d or not. Fine at this size; per-module loading is the upgrade
  when it stops being fine.

## Deferred, for discussion

- **A surface spelling for `0`.** ⊤'s value is settled as `(&)` (above).
  `0` has no values, and its consumer stays `select Empty {}`; whether the
  empty sum deserves an anonymous type spelling is still open.

