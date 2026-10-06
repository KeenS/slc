# Sections and files

```sl
sect geometry {
    pub enum Shape { Circle(i64), Rect(i64, i64) }
    func squared(n: i64) -> i64 { <(n, n) | mul }
    pub func area(s: Shape) -> i64 { … }
}

cite geometry::area;
```

A declaration in a section is private unless it is `pub`. Private means the
section and the sections nested in it. A declaration in no section is
visible in the whole program. `main` is declared at the root.

A path resolves by its first segment. `geometry::Shape::Circle` is refused
when `Shape` is private. Inside the section, its own names are bare.

## cite

| Form | What it brings |
|---|---|
| `cite geometry::area;` | That name, bare |
| `cite geometry::*;` | Every public member, bare |
| `cite list::List::*;` | The public variants |
| `cite Colour::{Red, Blue};` | Those variants |

A cite does not run the name. Imports belong to the file that wrote them. A
glob loses to an explicit cite and to a declaration in the same section. Two
globs that offer one name are reported at the use. Two explicit cites of the
same name are an error at the cite.

`cite list;` cites a section that is already reachable, which is allowed.
A program section whose name is a library section shadows that library
section.

Citing variants does not cite the type. A signature that writes `List<i64>`
also has `cite list::List`, or it uses the path `list::List`.

## Files

```sl
sect geometry;
pub sect report;
```

The file contains the declarations, without a wrapping `sect`. The directory
tree is the section tree.

| Declared in | `sect name;` is the file |
|---|---|
| `dir/main.sl` | `dir/name.sl` |
| `dir/m.sl` | `dir/m/name.sl` |
| inline, inside `sect a` | one `a/` directory further down |

Run and check the root. There is no search path. The sample is
[`examples/programs/multi_file/`](https://github.com/KeenS/slc/tree/master/examples/programs/multi_file).

Resolution flattens sections before checking. The checker sees qualified
names. A library file is loaded when a path or a cite names it, and the
prelude is loaded always.
