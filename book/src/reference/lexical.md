# Lexical structure

A source file is Unicode text. The lexer reads one file at a time, so a
string or a comment left open is reported in the file that opened it.

## Comments

```sl
// to the end of the line
/* block comments nest /* like this */ */
```

Comments and blank lines are kept by `slc fmt`.

## Identifiers

An identifier starts with a letter or `_` and continues with letters,
digits, or `_`. Letters are Unicode letters.

`return`, `if`, `else`, `true`, and `false` are ordinary identifiers. `True`
and `False` are the variants of `Bool`. In a handler clause, `return`
followed by `(` is the body's result: `return(x) => e`.

## Keywords

These words are tokens:

| Word | Role |
|---|---|
| `func`, `proc`, `fn` | Functions. `func` names one, `fn` is a lambda, `proc` is a command |
| `data`, `enum`, `menu`, `form` | Type declarations |
| `spec`, `impl`, `for` | Traits |
| `hook`, `do`, `hand`, `hn`, `reset` | Effects and handlers |
| `of`, `mu`, `let` | Matching, consumers, binding |
| `sect`, `cite` | Sections |
| `def`, `pub`, `dual` | A constant, visibility, the dual of a type |

`let+` and `let-` are `let` with a sign touching the keyword.

An older spelling is a parse error that names the current one.

| Older spelling | Current spelling |
|---|---|
| `fn` on a named declaration | `func` |
| `command` | `proc` |
| `trait` | `spec` |
| `effect` | `hook` |
| `const` | `def` |
| `match` | `of` |
| `handle` | `do` |
| `handler` | `hn` |
| `op` | `hn` |
| `with` | `do` |
| `select` | `mu` |
| `mod` | `sect` |
| `use` | `cite` |

The lambda stays `fn`. The migration note is
[`docs/MIGRATION.md`](https://github.com/KeenS/slc/blob/master/docs/MIGRATION.md).

## Literals

| Literal | Type when unconstrained | Example |
|---|---|---|
| integer | `i64` | `42`, `1_000`, `-7` |
| float | `f64` | `1.5`, `-0.5` |
| string | `String` | `"hello\n"` |
| character | `char` | `'A'`, `'\n'` |

An integer or float literal takes a narrower type when the port it meets
requires one. A minus sign is part of the literal only when it touches the
digits. `_` inside a number is ignored. A number that does not fit in 64
bits is a lex error.

String and character escapes are `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, and
`\'`. An unknown escape is a lex error.

## Punctuation

Arrows: `->`, `<-`, `=>`, `<=`. Paths: `::`. Ranges in patterns: `..=`.
The flow operators are `<`, `|`, and `>`. Type and effect rows use `/` and
`..`, as in `/ {IO}` and `..E`.

`+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<` as a comparison, `<=` as a
comparison, `>=`, `>`, `&&`, `||`, and `!` are recognized so the parser can
name the function or the `of` that replaces them. `<=` is a comparison only
where a pattern range or a copattern arrow is not being parsed; the
comparison token `<=` is `Le`, and the copattern arrow is the same spelling
in arm position. Writing `a <= b` as a comparison is the error that names
`le`.
