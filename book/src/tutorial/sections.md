# Sections

A `sect` is a named scope. `::` reaches into it. `cite` brings a name into
the current scope and does not run it. A declaration inside a section is
private unless it is marked `pub`. A declaration in no section is visible
everywhere, which is why a single-file program can ignore sections and why
the prelude is the prelude.

```sl
{{#include ../../examples/modules.sl}}
```

```text
75
42
```

Inside `geometry`, `Shape` and `squared` are bare names. From `main`, the
enum is `geometry::Shape` and the public function can be cited:

```sl
cite geometry::area;
<geometry::Shape::Circle(5) | area | println;
```

`squared` has no `pub`, so `geometry::squared` from `main` is refused.
Privacy is the section and the sections nested in it.

`cite geometry::Shape::*;` brings the public variants in bare. `cite
geometry::*;` brings every public member. A glob is the weakest way a name
arrives: an explicit cite and the section's own declarations win. Two globs
may offer the same name, and that is reported when the name is used.

A program's declaration of a prelude name shadows the prelude name.

## A section in its own file

A `sect` with no body names a file:

```sl
sect shapes;
pub sect report;
```

The file holds the declarations themselves, with no `sect shapes { … }`
around them. The directory tree is the section tree.

| The declaration is in | `sect name;` reads |
|---|---|
| the program, `dir/main.sl` | `dir/name.sl` |
| a section file, `dir/m.sl` | `dir/m/name.sl` |
| an inline section `sect a { … }` | one `a/` further down |

There is no search path. Run and check the root file. The sample is
[`examples/programs/multi_file/`](https://github.com/KeenS/slc/tree/master/examples/programs/multi_file).

`main` is declared at the root. A `main` inside a section is `that::main`,
and the entry point does not accept it.

The standard library is a set of sections, one per file: `list`, `map`,
`fs`, and the rest. Nothing in them is in scope until the program names it.
The next chapter does.
