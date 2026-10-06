# How to read this reference

The reference lists the surface the compiler accepts. Short programs in the
tutorial are included from `book/examples/` and checked by `book/check.sh`.
A fragment in the reference shows a form and leaves the surrounding program
out; the specification's complete programs live in
[`docs/design/`](https://github.com/KeenS/slc/tree/master/docs/design) and
are compiled by the test suite.

A signature is written the way a declaration writes it. A call supplies the
value group by flowing into the function, as [Flow](flow.md) describes.

The pages are:

| Page | What it answers |
|---|---|
| [Lexical structure](lexical.md) | Tokens, literals, keywords |
| [Declarations](declarations.md) | `func`, `proc`, `data`, `enum`, `menu`, `form`, `spec`, `hook`, `hand`, `def`, `sect` |
| [Expressions](expressions.md) | Blocks, `of`, `mu`, `do`, `let`, chains |
| [Types](types.md) | Primitives, arrows, connectives, rows, signs |
| [Patterns](patterns.md) | What an arm or a `let` may match |
| [Flow](flow.md) | `<`, `\|`, `>`, application, cuts |
| [Polarity](polarity.md) | When a computation runs |
| [Effects and handlers](effects.md) | `hook`, `do`, `hn`, `hand`, rows |
| [Traits](traits.md) | `spec`, `impl`, bounds, associated types |
| [Sections and files](sections.md) | `sect`, `cite`, files |
| [Prelude](prelude.md) | Names in every program |
| [Standard library](library.md) | Sections cited by path |
| [Builtins](builtins.md) | Operations the compiler provides |
| [The slc command](cli.md) | `run`, `check`, `fmt` |
| [Diagnostics](diagnostics.md) | Phases and runtime failures |
| [Surface forms](forms.md) | The shapes, gathered in one place |
