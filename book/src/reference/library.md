# Standard library

The library is
[`crates/slc-driver/src/stdlib/`](https://github.com/KeenS/slc/tree/master/crates/slc-driver/src/stdlib).
Each file is a section named after the file. A program reaches it by path
or by `cite`. Only the sections a program names are loaded.

Collections are persistent. An update returns a new value and leaves the
old one unchanged. `List`, `Seq`, and `Stream` are three types: eager data,
a finite menu of steps, and a coinductive menu.

## list

```sl
pub enum List<+T> { Nil, Cons(T, List<T>) }
```

| Operation | Signature |
|---|---|
| `length` | `(List<T>) -> i64` |
| `append` | `(List<T>, List<T>) -> List<T>` |
| `map` | `(A -> B / {..E}, List<A>) -> List<B> / {..E}` |
| `range` | `(i64, i64) -> List<i64>`, inclusive, empty when `from` is greater than `to` |
| `filter` | `(T -> Bool / {..E}, List<T>) -> List<T> / {..E}` |
| `fold` | `(List<A>, B, (B, A) -> B / {..E}) -> B / {..E}` |
| `sum` | `(List<i64>) -> i64` |
| `reverse` | `(List<T>) -> List<T>` |
| `take`, `drop` | `(List<T>, i64) -> List<T>` |
| `nth` | `proc (List<T>, i64) \| (found: T & missing: String)` |

A non-positive `take` is the empty list, and a non-positive `drop` is the
whole list. A count past the end takes the whole list or drops to nothing.
`range` of a single `i64`, including the greatest one, is that one element.
`Display` renders `[1, 2, 3]`.

## map and set

`map::Map<K, V>` is an AVL tree. Keys use `Ord`. Two keys are the same when
neither is less, so a `NaN` key collides with the node the search reaches.
`set::Set<K>` is that tree with nothing stored beside the key. `Display` for
a map is `{a: 1, b: 2}`, and for a set `{1, 2, 3}`, in key order.

| Operation | On `Map<K, V>` | On `Set<K>` |
|---|---|---|
| `empty` | `() -> Map<K, V>` | `() -> Set<K>` |
| `length` | the number of entries | the number of keys |
| `insert` | `(map, K, V) -> Map` | `(set, K) -> Set` |
| `remove` | `(map, K) -> Map` | `(set, K) -> Set` |
| `contains` | `(map, K) -> Bool` | `(set, K) -> Bool` |
| `of_list` | a list of pairs, later pairs winning | a list of keys |
| `to_list` | the pairs, in order | the keys, in order |
| `get` | `proc (map, K) \| (found: V & missing: String)` | |

`num::min` and `num::max` use the same `Ord`. When the two arguments compare
equal they answer the second.

Both have a `Builder` menu. `builder()` starts empty. `put` on a map takes
the key and the value and answers the next builder. `put` on a set takes the
key. `finish` answers the collection. `map::put` is also the function a
chain flows into; `builder.put` is that function demanded from the menu.

## hashmap and hashset

`hashmap::HashMap<K, V>` is a 4-way hash trie. Keys use `Hash` and `Eq`, and
equal keys hash equal. `hashset::HashSet<K>` stores nothing beside the key.
The operations match `map` and `set`, with `Hash` and `Eq` as the key bounds.
`HashMap`'s `get` is the same `found` / `missing` command. `HashSet` has
`contains` and no `get`. Builders match as well: a hash map's `put` takes
the entry, and a hash set's `put` takes the key.
`Display` uses the same braces as the ordered collections. The order is the
trie's order.

## array

`array::Array<T>` is an immutable 4-way trie. `array::Array4<T>` is one
branch, `One` through `Four`, every slot occupied.

| Operation | Signature |
|---|---|
| `empty` | `() -> Array<T>` |
| `length` | `(Array<T>) -> i64` |
| `push` | `(Array<T>, T) -> Array<T>` |
| `of_list`, `to_list` | between `Array<T>` and `list::List<T>` |
| `get` | `proc (Array<T>, i64) \| (found: T & missing: String)` |
| `update` | `proc (Array<T>, i64, T) \| (updated: Array<T> & missing: String)` |
| `slots` | `(Array4<T>) -> i64` |
| `slot_get` | `proc (Array4<T>, i64) \| (found & missing)` |
| `slot_update` | `proc (Array4<T>, i64, T) \| (updated & missing)` |

`Display` for an array is the bracket form used for lists. `push` grows a
new level when a branch is full, at 4, 16, and 64 elements.

## string

```sl
menu Builder {
    append(part: String): Builder,
    finish: String,
}
func new() -> Builder
func push<+T: Display>(builder: Builder, value: T) -> Builder
```

`new()` is the empty builder. `append` and `finish` are fields, so
`builder.append(part)` and `<part | builder.append` are the same demand.
`push` renders through `Display` and appends. Each builder closes over one
string, so a shared prefix can diverge.

## option and either

```sl
enum Option<+T> { None, Some(T) }
func unwrap_or<+T>(o: Option<T>, fallback: T) -> T

enum Either<+L, +R> { Left(L), Right(R) }
```

`Either` has no methods. Neither side means success.

## num

| Operation | Signature |
|---|---|
| `min`, `max` | `<+T: Ord>(T, T) -> T`. Equal arguments answer the second |
| `abs`, `signum` | `(i64) -> i64` |
| `is_even`, `is_odd` | `(i64) -> Bool` |
| `gcd`, `lcm` | `(i64, i64) -> i64` |
| `div_rem` | `(i64, i64) -> (i64, i64)` |

`abs` here is the `i64` function. The float method is the prelude's `Abs`.

## stream

```sl
menu Stream<+T, E> / {..E} {
    head: T,
    tail: (-> Stream<T, ..E> / {..E}),
}
```

| Operation | What it answers |
|---|---|
| `repeat` | the same value at every demand |
| `count_from` | `n`, then `n + 1`, then onward |
| `iterate` | `x`, then `f(x)`, then onward |
| `unfold` | each step answers an element and the next seed |
| `map` | `f` on each demanded element |
| `zip` | pairs of demanded elements |
| `drop` | the stream after `n` demands, performed while `drop` runs |
| `take` | the first `n` elements as a `list::List` |

There is no `Display` for `Stream`. Showing a prefix is `take`. The row is
performed when a step is demanded. The argument that passes the stream on is
a by-name computation, `(-> Stream<T, ..E> / {..E})`, so the stream is
rebuilt at each demand.

## seq

```sl
enum Step<+T, E> { Done, Yield(T, Seq<T, ..E>) }
menu Seq<+T, E> / {..E} { next: Step<T, ..E> }
```

| Operation | Signature |
|---|---|
| `of_list` | `(List<T>) -> Seq<T>` |
| `to_list` | `(Seq<T, ..E>) -> List<T> / {..E}` |
| `of_stream` | a by-name `Stream` becomes a `Seq` |
| `map`, `filter` | step the sequence, performing the function's row on demand |
| `take` | `(Seq<T, ..E>, i64) -> Seq<T, ..E>` |
| `take_while` | a stream, cut where the predicate returns `False`, as a `Seq` |

`filter` over an infinite stream is a finite program when a later `take`
stops asking. There is no `Display` for `Seq`. Showing one is `to_list`.

## lazy

```sl
menu Lazy<*T, E> / {..E} { force: T }
func of_delayed<-T, E>(computation: (-> T / {..E})) -> Lazy<T, ..E>
func to_delayed<-T, E>(computation: Lazy<T, ..E>) -> (-> T / {..E})
```

`force` runs the computation under the handlers around the demand.
`of_delayed` and `to_delayed` do not run it and do not cache it.

## fs

`fs` performs the `Fs` effect. `do expr fs::real` answers it from the disk.
The hand's clauses perform `IO`. Closing a `File` is a convention: a later
read through a closed handle fails at run time.

| Operation | Outcomes |
|---|---|
| `read` | `proc (path) \| (ok: String & failed: String)` |
| `write` | `proc (path, contents) \| (ok: unit & failed: String)` |
| `open` | `proc (path) \| (opened: File & failed: String)` |
| `read_line` | `proc (file) \| (line: String & end: unit)` |
| `close` | `(File) -> (,) / {Fs}` |
| `exists` | `(String) -> Bool / {Fs}` |

```sl
{{#include ../../examples/fs.check.sl}}
```

[`examples/programs/file_io.sl`](https://github.com/KeenS/slc/blob/master/examples/programs/file_io.sl)
is a complete program, including closing the handle on the way out.

## args and clock

```sl
func arguments() -> List<String> / {Args}
func now() -> i64 / {Clock}
```

`do expr args::real` answers with the words after the source file, in order.
The runtime path, the file path, `--fuel`, and `--interpret` are not among
them. An empty argument list prints `[]`.

`do expr clock::real` answers a monotonic nanosecond count. The origin is
arbitrary and local to the process. The difference of two readings is the
time between them, independent of the civil clock. A count that does not fit
in `i64` is an arithmetic overflow.

Both hands perform `IO`, so `main` still leaves `IO`.

```sl
{{#include ../../examples/args.sl}}
```

```text
[]
```

```sl
{{#include ../../examples/clock.sl}}
```

```text
true
```

`slc run book/examples/args.sl one two` prints `[one, two]`.

## control

```sl
hook Shift<+A, +R, E> {
    func shift(callback: (-> ((A -> R / {..E}) -> R / {..E}) / {..E})) -> A;
}
hand reset { shift(callback): resume => <resume | callback }
```

`do expr control::reset` handles `Shift`. One installation has one answer
type `R`. A resumption runs the captured continuation afresh and keeps that
handler's residual row. The example is
[`examples/effects/composable_capture.sl`](https://github.com/KeenS/slc/blob/master/examples/effects/composable_capture.sl).

## trace

```sl
proc tap<+T: Display, E>(label: String, x: T) | (k: (-T / {..E})) / {IO, ..E}
```

`tap` prints the label and the value, then sends the value to `k`.
