# The library

The prelude is in every program. The standard library is one section per
file, and a name from it is a path or a cite. This program cites the list
operations it uses and reaches `map`, `string`, and `option` by path.

```sl
{{#include ../../examples/library.sl}}
```

```text
[1, 2, 3, 4, 5]
15
[2, 4]
[5, 4, 3, 2, 1]
{a: 1, b: 2}
Hello, SLC!
3
0
```

## Lists

`list::List<T>` is the enum `Nil` and `Cons(T, List<T>)`. `cite list::List`
brings the type in as `List`. `cite list::List::*;` brings `Nil` and `Cons`.
Citing the variants does not bring the name `List`, so a signature that
writes `List<i64>` cites the type as well, or writes `list::List<i64>`.

`<(1, 5) | range` is the inclusive list `[1, 2, 3, 4, 5]`. `sum` adds it.
`filter` keeps the elements for which the function returns `True`. `reverse`
reverses. `length`, `append`, `map`, `take`, `drop`, and `fold` are the same
kind of function. `nth` can miss, so it is a `proc` with `found` and
`missing`.

`Display` for a list is the bracket form printed above. The elements use
their own `Display`.

## Maps and builders

`map::Map<K, V>` is a persistent ordered map, an AVL tree. `insert` and
`remove` return a new map and leave the old one unchanged. Keys use `Ord`.
Two keys are the same when neither is less. `get` offers `found` and
`missing`.

```sl
let m = <map::empty() | x => (x, "a", 1) | map::insert | y => (y, "b", 2) | map::insert;
```

The binder `x =>` names the map so far and passes the triple `insert`
expects. The printed form is `{a: 1, b: 2}`.

`string::new()` is a persistent string builder, a menu. `append` answers the
next builder, and `finish` answers the `String`. `b.append("Hello")` demands
that field. `<", SLC!" | b.append` is the same demand written as a chain.
`string::push` renders any `Display` value and appends it.

`option::Option<T>` is `None` or `Some(T)`. `unwrap_or` returns the carried
value, or the fallback when the option is `None`. `either::Either<L, R>` is
`Left` or `Right`, and neither side means success.

The [library reference](../reference/library.md) lists every public operation:
hash maps and sets, arrays, streams, sequences, files, arguments, and the
clock.
