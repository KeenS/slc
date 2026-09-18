# Modules in files of their own

Status: implemented. `DESIGN.md` §10 is the definition; this note records
what was chosen and why.

## Why

`slc run` took one source. `mod` named a scope but could not name a file, so
a program grew only downward — `examples/programs/json_parser.sl` is 350
lines for want of anywhere else to put them — while the library, which *is*
several files, was reachable only because it is compiled into the driver.

## What a file is

A `mod` with no body names a file:

```sl
mod geometry;          // the declarations of `geometry` are in geometry.sl
```

The file holds the module's *body*, not a `mod geometry { … }` around it: the
name is the declaration's to give, once, where the module is declared. `pub
mod geometry;` is public as `pub mod geometry { … }` is. Everything else
about the module — privacy, paths, `use`, globs — is §10's, unchanged: a
module in a file is a module.

The standard library uses the same body-only convention. `stdlib/list.sl` is
loaded as if it were `mod list { … }`, and `stdlib/string.sl` as if it were
`mod string { … }`. `prelude.sl` is the exception: it is appended at the root
because its declarations are visible to every program without an import.

## Which file

Rust's rule, because the surface is Rust-flavoured and the rule is known:

| declared in                  | `mod name;` is             |
|------------------------------|----------------------------|
| the program, `dir/main.sl`   | `dir/name.sl`              |
| a module file, `dir/m.sl`    | `dir/m/name.sl`            |
| inline, inside `mod a { … }` | one `a/` further down      |

So the directory tree is the module tree, and a path in the source says
where to look. There is no search path, no `mod.sl`, and no way to name a
file elsewhere: a program is the files under its own directory. A file
reached twice — two `mod name;` in one scope — is refused rather than
declared twice.

## How it is compiled: a unit, spliced

The library already showed the shape. It is *source units* appended to the
program's text, each through the same pipeline as user code, with a source
map naming the unit a span falls in and resolution scoping variant imports
by unit. A module file is one more unit, between the program and the
library.

What differs is where its declarations belong: not at the root, but inside
the `mod` that named it. Library files use the same synthetic wrapper: every
library unit except the prelude is loaded as `mod <file-stem> { … }`. So the
driver works on tokens. Each unit is lexed **on its own**, its spans
shifted to where its text sits in the combined source; then the `;` of
`mod name;` is replaced by `{`, the file's tokens, and `}`. The parser sees
one ordinary program — which matters, because it reads every `menu` in the
token stream before parsing to tell `mu M { item <= c }` from a binder, and
a menu declared in one file is used in another.

Alternatives considered:

- *Parse each file, then graft the trees.* The menu pre-pass would see one
  file at a time, and every span in a grafted tree would need shifting; a
  token's span is one field in a flat list.
- *Wrap the file's text in `mod name {` … `}` and concatenate.* The wrapper
  shifts the file's first line, so every diagnostic on it is a column out.

Lexing units apart also closes a hole for good. Lexed as one text, a string
the program left open closed on the prelude's first quote; the fix at the
time was to check the program's syntax alone first. Now no unit's lexing
can see another's text, and each file's syntax is checked alone as it is
loaded, so an unclosed brace is reported in the file that left it open.

## What a diagnostic says

A span in the program's own file is `line:column`, as before. In any other
unit it is `file:line:column` — the library's `prelude.sl:12:3`, and a
module's path as the driver found it, `src/geometry.sl:4:9`. A `mod name;`
whose file cannot be read is reported at the declaration, with the path
that was tried.

## What is not decided here

`slc check` and `slc fmt` take files. `fmt` is per file and needs nothing:
alone, `mod name;` parses as an empty module. `check` of a *module* file
checks it as a program of its own, which fails wherever the module reaches
for a name its parent supplies; check the root. Whether `check` should find
a file's root is open, and so is any notion of a package.
