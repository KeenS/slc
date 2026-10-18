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

The lambda stays `fn`. A named function is `func`. The migration note is
[`docs/MIGRATION.md`](https://github.com/KeenS/slc/blob/master/docs/MIGRATION.md).
Words it records, such as `command` and `mod`, are ordinary identifiers.

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
The flow marks are `<`, `|`, and `>`. Type and effect rows use `/` and
`..`, as in `/ {IO}` and `..E`. `+`, `-`, and `*` are signs on a type, and
`+` and `-` also mark `let+` and `let-`. A `-` that touches a number is part
of that literal. `@` binds a pattern: `name @ pattern`.

`<=` is the demand arrow, in a `mu` arm and in a chain binder `k <= e`.
`!`, `%`, `==`, `!=`, `>=`, `&&`, `||`, and the brackets `⟨` `⟩` are not
tokens.
