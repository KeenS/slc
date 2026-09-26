Part of the [language design](../../DESIGN.md).

## 9. Entry point and exit

A program is a command, so its entry point is a `proc`. It takes no values
and exactly one continuation — the exit status:

```sl
proc main | (exit: i32) / {IO} {
    <"Hello, SLC!" | println;
    <0 | exit>
}
```

The runtime supplies that continuation; the cut that reaches it is what ends
the program, and the integer it carries is the process exit status.

`exit` is a parameter, and it is the *only* door out. There is no global exit
consumer: ending the program is a right a helper is handed — as a continuation
parameter, or captured into a consumer built where `exit` is in scope — never
one it takes for itself. (An earlier design had a top-level `EXIT`; it let any
function end the program behind `main`'s back, and it is gone.)

```sl
proc main | (exit: i32) / {IO} {
    let complain = mu String { message => { <message | println; <1 | exit> } };
    <(,) | fs::real_command | (fn {
        <"input.txt" | fs::read | (mu String { text => { <text | print; <0 | exit> } } & complain)>
    })>
}
```

Because a `proc` body must be `⊥`, **every terminating path of a program
leaves through `exit`** — a `main` that falls off the end is rejected by the
type checker, not by a runtime convention.

There is no final-result value. A program's output is exactly what it prints;
its status is what it sends to `exit`. A `func main`, a `main` with value
parameters, a `main` whose row is not one exit status, and a missing `main`
are all rejected.

The evaluator runs on a dedicated stack, so how deeply a continuation-passing
program nests is bounded by memory rather than by the host's default stack.

## Literals

A lambda whose body ends in a cut produces nothing, so it *is* a consumer:
`fn(message: +String) -> (;) { … }` has type `-String`, and may be written
wherever a consumer of a `String` is expected. `A → ⊥` and `-A` are the same
type, not two that convert, so a continuation parameter may be annotated
either way.

The `-> (;)` may be omitted — the body decides the type — but the examples
write it, because a consumer literal is worth reading as one at a glance.

`A -> B` is `(-A ; B)`, which is why this works: `A -> (;)` is `(-A ; (;))`,
and `(;)` is the unit of `;`. The same identity gives the dual: `dual(A -> B)`
is `(A, -B)`,
an argument together with a continuation for the result — a *call stack*. So
`<v | f` delivered to `k` and the cut of `f` against `(v, k)` are the same interaction,
and a consumer of a function is an ordinary value of that product type.

A cut is well typed exactly when its two sides are dual. Which side is
written negatively is not itself the question: `v | k` sends `v` to something
that consumes it, and for a function that something is a call stack.

An integer literal takes the integer type its port requires — `<0 | exit>`
sends an `i32` — and is `+i64` when nothing constrains it. The integer
primitives are `i8`, `i32`, `i64`, `u8`, `u32`, and `u64`; a floating-point
literal similarly takes `f32` or `f64` from its port and is `+f64` when
unconstrained. Every other value must match its port exactly: there is no
implicit widening or narrowing of a value that is not a literal. A
conversion between widths is the prelude's `Into<+U>`, one impl for every
pair of the six integer widths. The expected type selects the destination.
The number is kept when it fits there; a value that does not fit is an
arithmetic overflow, as `add` overflowing is. There is no truncating cast.
Every integer is one signed word, so a `u64` reaches as far as `i64` does.

The numeric primitives have the `Display`, `Add`, `Sub`, `Mul`, `Div`, `Rem`,
`Eq`, and `Ord` implementations supplied by the prelude; signed integers and
floats also have `Neg`. A floating-point literal does not coerce to an integer
type, and an integer literal does not coerce to a floating-point type. Numeric
patterns, including inclusive ranges, take the numeric type of their
scrutinee. A string literal is `+String`, a character literal is
`+char`. `True` and `False` are not literals but the
variants of the prelude's `enum Bool`. `(,)` is the
unit value, of type `(,)`; an empty block is the same.

## Diagnostics

Compiler failures are categorized by the phase that produces them:

| Category         | Meaning                                                                              |
|------------------|--------------------------------------------------------------------------------------|
| `parse`          | the source is not a valid surface program                                            |
| `type`           | a term has the wrong type or an inference rule cannot apply                          |
| `polarity`       | a value or continuation is used with the wrong polarity                              |
| `exhaustiveness` | an `of` or `mu` does not cover its alternatives exactly once                   |
| `lowering`       | an otherwise accepted surface construct cannot be translated to the core calculus    |

The compiler applies these phases in order:

1. `parse`
2. `type`
3. `polarity`
4. `exhaustiveness`
5. `lowering`

A phase stops before later phases once it reports a diagnostic. Consequently,
`parse` diagnostics take precedence over all checker diagnostics; `type`
diagnostics take precedence over polarity and exhaustiveness; and
so on. Within one phase, diagnostics are source-ordered. Every diagnostic
names its `line:column` and quotes the source it is about — a lex or parse
error as a checker's does: `parse error: there is no `+` operator … (at 3:8
`+`)`. A syntax error's span can run to the end of the file, as an
unterminated string's does, so only its first line is quoted; and one with no
extent, at the end of input, names no place rather than a wrong one.

Every source unit — the program's file, each module file, each library
file (§10) — is lexed on its own, and a program's files have their syntax
checked on their own. Read as one text, a string the program left open
would close on the prelude's first quote, and a brace on its last, and the
error would be reported in a file the author did not write.

Runtime failures are not compiler diagnostics. They are reported after
evaluation begins and do not participate in this precedence order.

`slc check <file.sl>...` applies the phases and stops: it reports what `run`
would report before evaluating, and evaluates nothing, so a program that
would not stop can still be checked. Each diagnostic is prefixed with its
file, and one failing file fails the command. A file that declares no `main`
is a library and checks; a `main` of the wrong shape is refused as `run`
refuses it (§9).

Some slips get a message of their own rather than the mismatch they cause. A
handler clause binding the wrong number of parameters reports the operation's
arity and the number bound; a clause returning a different type reports the
handler's answer type. An incomplete effect handler lists the missing
operations and suggests a final `_ => forward` when forwarding is intended.
A function closed with `>` — `<42 | resume>` in a handler's clause, where
`<42 | resume` was meant — is reported as a function applied by leaving the
`>` off, instead of as a value meeting the argument-and-continuation pair a
function takes by a cut. A handler clause naming no operation of any
effect — `fs::nope(path): resume => …` — is refused by name, where it would
otherwise be ignored and leave its effect reported as unhandled.

## 10. Modules

A `mod` is a named scope of declarations, `::` reaches into it, and `use`
brings one name into scope:

```sl
mod geometry {
    pub enum Shape { Circle(i64), Rect(i64, i64) }

    func squared(n: i64) -> i64 { <(n, n) | mul }   // private: the module's own

    pub func area(s: Shape) -> i64 { … }    // its own names are bare here
}

use geometry::area;

proc main | (exit: i32) / {IO} {
    <geometry::Shape::Circle(5) | area | println;
    <0 | exit>
}
```

**A declaration inside a module is private unless it is `pub`.** Private
means reachable by that module and the modules nested inside it, and nowhere
else — so a module's helpers are not part of its surface. A declaration in
no module is visible everywhere, which is what lets the prelude be the
prelude and leaves a single-file program unaffected.

The rule is enforced after flattening, where both halves are known: a
reference is a qualified name, and the declaration it sits in carries the
module it was written in. A reference reaches the longest declared prefix of
the path it names, so `geometry::Shape::Circle` is refused when `Shape` is
private, not only when some `Circle` is.

The library's modules are declared in their own source units, appended
after the program. Every library file except `prelude.sl` is implicitly
wrapped in a module named after its file stem. Imports are scoped to the unit
that wrote them: a
library file's `use Enum::*;` pins names in that file only, and a program's
imports never reach into the library. The root scope, though, is one scope
over every unit, so a library unit imports names only inside its `mod` —
at its top level a `use` may be a variant import, which is per-unit, and
nothing else. A program's `mod` of a library module's name shadows it
whole, as its `func` shadows a prelude function. `use list;` — naming a
module already reachable at the root — is allowed, so a program can say
what it draws on.

`use m::*;` brings every `pub` member of module `m` in bare — a glob. It is
the weakest way a name arrives: an explicit `use m::f;` and the importing
module's own declarations both win over it. Two globs may bring the same
name, and that is not an error until the name is used — `use list::*; use
seq::*;` is fine, and a bare `map` after it says it could be either and asks
for the one you mean. A glob over an enum, `use list::List::*;`, still
brings its variants, as before.

Modules exist only to resolution, which runs right after parsing: every
declaration inside `mod m` is renamed `m::name`, every reference is rewritten
to the qualified name it resolves to, and the `mod` and `use` declarations
disappear. The checker, the lowering, and the runtime never see them — they
work on flat names, which always contained `::`, because an enum variant is a
path already.

A name resolves in scope order: a local binding shadows everything and is
left alone; then the enclosing module's `use` aliases; then its own
declarations; then each ancestor's, out to the root. A path resolves by its
first segment and keeps the rest, so `inner::deep()` works from a sibling and
`geometry::Shape::Circle` from anywhere. A name nothing claims is left bare
for later passes — that is how builtins stay global. Two `use` declarations
bringing in the same name are an error.

Visibility follows the private-by-default rule above, and `main` must be
declared at the root — a `main` inside a module is `m::main`, which the entry
point does not accept.

### A module in a file of its own

A `mod` with no body names a file that holds the module's body:

```sl
mod geometry;          // the declarations of `geometry` are in geometry.sl
pub mod report;        // public, as `pub mod report { … }` is
```

The file holds the declarations themselves, with no `mod geometry { … }`
around them: the name is given once, where the module is declared. Nothing
else about the module changes — privacy, paths, `use` and globs are as
above — because a module in a file is a module.

The directory tree is the module tree:

| declared in                  | `mod name;` is         |
|------------------------------|------------------------|
| the program, `dir/main.sl`   | `dir/name.sl`          |
| a module file, `dir/m.sl`    | `dir/m/name.sl`        |
| inline, inside `mod a { … }` | one `a/` further down  |

There is no search path and no way to name a file elsewhere: a program is
the files under its own directory. A `mod name;` whose file is missing is
reported at the declaration with the path that was tried, and a file reached
twice is refused rather than declared twice.

A module file is a *source unit*, as each library file is. It is lexed on
its own — so nothing one file leaves open can close in another — and its
syntax is checked on its own; then its tokens take the place of the `;`,
between a `{` and a `}`, and the parser reads one ordinary program. That is
why a `menu` declared in one file is known in the others
([§7](data.md#7-additive-data)), and why
imports behave as they do across files: a variant import is scoped to the
unit that wrote it, so a file's `use Shape::*;` pins bare names for that
file alone. A diagnostic in the program's own file is `line:column`; in any
other unit it names the file, `src/geometry.sl:4:9`.

`slc run` and `slc check` take the program — the root. `slc fmt` is per file,
and formats a module file like any other. `docs/design-notes/file-modules.md`
records the alternatives.

### Variant imports

`use` has three forms:

```sl
use module::name;        // one member, aliased into this module
use Colour::*;           // every variant of an enum, bare
use Colour::{Red, Blue}; // the listed variants, bare
```

A bare variant name resolves in this order: an explicit import pins it — an
import that collides with another import, or names a variant its enum does
not have, is an error at the `use` — otherwise the automatic rule applies:
unqualified while exactly one enum declares the name. A bare name that
*several* enums declare and nothing imports is an **error**, not a binder:
a pattern that silently caught everything is the failure mode this rule
exists to kill. Imports are **scoped to their source unit**: the prelude
pins its own bare names with `use List::*;`, and that import reaches no
program code — just as a program's `use Mine::*;` never changes what the
prelude means, and the two never collide.
