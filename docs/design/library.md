Part of the [language design](../../DESIGN.md).

## Standard library

The library has two layers, and both are ordinary SLC source that goes
through the same pipeline as user code.

**The prelude** (`crates/slc-driver/src/prelude.sl`) is what every program
sees unasked: the `hook IO` the runtime handles, and the
**`Display` trait** (`func fmt(self: Self) -> String`, user-facing formatting
as in Rust) with impls for `i64`, `String`, `Bool`, and the unit, tuples and
choices up to eight components, rendered as they are written — and
`to_string<T: Display>`; `enum Bool { False, True }`, the type every yes-or-no
answer has; arithmetic and comparison as the traits `Add`, `Sub`, `Mul`,
`Div`, `Rem`, `Neg`, `Eq` and `Ord`, with impls for the base types; `Into<U>`,
moving a value between the integer widths and `f32` and `f64`; `Sqrt`, `Abs`,
`Floor`, and `Ceil` on `f32` and `f64`; `wrapping_mul` and `xor` on the
machine word; `char_to_code`, a character's scalar value; `Hash`,
answering a non-negative `u64` for the integer widths, `char`, `Bool`, and
`String`; `index`,
a `String`'s character at a position; and `not`, which negates a `Bool`,
since there is no `!`. A
program's own declaration of a prelude name shadows it.

**The stdlib** (`crates/slc-driver/src/stdlib/`) is one module per file,
appended after the prelude, and nothing in it is in scope until named: a
module is reached by its path, `list::length`, or a name is brought in bare
with `cite`. Each file supplies the body of the module named by its file stem,
so `stdlib/list.sl` is loaded as `sect list { … }`; the source does not repeat
that wrapper. `prelude.sl` is the root-scope exception. Each module marks what
it offers `pub`; the rest is its own.

| module | what it offers |
|---|---|
| `list` | `List<T>`, `length`, `append`, `map`, the outcome-offering `proc nth`, and the eager `range`, `filter`, `fold`, `reverse`, `take`, `drop`, and `sum` — and `impl<+T: Display> Display for List<T>`, which lives with the type and is found from anywhere (`[1, 2, 3]`) |
| `map` | `Map<K, V>`, an AVL tree: `empty`, `insert`, `remove`, `contains`, `length`, `of_list`, `to_list`, the outcome-offering `proc get`, `Display` (`{a: 1, b: 2}`), and `Builder` |
| `set` | `Set<K>`, that tree with nothing beside the key: `empty`, `insert`, `remove`, `contains`, `length`, `of_list`, `to_list`, `Display` (`{1, 2, 3}`), and `Builder` |
| `hashmap` | `HashMap<K, V>`, a 4-way hash trie: the same operations, keyed by `Hash` and `Eq`, and `Builder` |
| `hashset` | `HashSet<K>`, that trie with nothing beside the key, and `Builder` |
| `array` | `Array<T>`, an immutable 4-way trie: `empty`, `push`, `length`, `of_list`, `to_list`, and the outcome-offering `get` and `update` (`[1, 2, 3]`). `Array4<T>` is one branch, with `slots`, `slot_get`, and `slot_update` |
| `string` | `Builder`, a persistent string builder expressed as a `menu`; `new`, `push<T: Display>` and its `append` and `finish` items |
| `option`, `either` | `Option<T>` with `unwrap_or`; `Either<L, R>`, `Left` or `Right` with neither meaning success. Either/or outcomes are additive, so they are enums whose consumers are `mu`s — a `form` would want every field at once |
| `num` | `min` and `max`, generic over `Ord`; `abs`, `signum`, `is_even`, `is_odd`, Euclidean `gcd` and `lcm`, and `div_rem`, all on `i64` |
| `stream` | `Stream<T>`, the coinductive mirror of `List`, with `repeat`, `count_from`, `iterate`, `unfold`, `map`, `zip`, `drop`, and `take` bridging back to data, since an infinite structure cannot print whole and showing `<(s, n) | take` is the honest form |
| `seq` | `Seq<T>`, the finite codata sequence between the two (below) |
| `lazy` | `Lazy<T, E>`, the explicit by-name thunk for either polarity; `of_delayed` and `to_delayed` convert negative-result computations to and from `(-> T / E)` |
| `fs` | files: `read`, `write`, `open`, `read_line`, `close`, `exists` — commands offering each outcome to its own continuation, performing the `Fs` effect — and `real`, the handler that answers it from the disk |
| `args` | `arguments`, the words after the program file, as `List<String>`, performing `Args`; `real` answers from the process |
| `clock` | `now`, a monotonic nanosecond count as `i64`, performing `Clock`; `real` answers from the runtime |
| `control` | `Shift<A, R, E>`, `shift` and the thunk-taking `reset`: typed, multi-shot composable capture with a positive answer type and explicit residual effects |
| `trace` | one **tap**, `proc tap(label, x) \| (k)`, which logs what passes through and forwards it: `<("answer", 42) \| trace::tap \| out>` |

`Map<K, V>` is that tree, and it is library code the way `List` is. Keys and
values are positive. A key is compared with `Ord`; two keys are the same when
neither is less. That is equality only when `Ord` is a total order: a `NaN`
compares that way with every float, so it collides with the node the search
reaches. `num::min` and `num::max` follow the same `Ord`: when the two
arguments compare equal they answer the second, and `min` of `NaN` and a
number answers the number when `NaN` is first and `NaN` when the number is first.
Each node stores its height, and `insert` and `remove` rebalance
until a node leans by at most one. Both answer a new map and leave the map
they were given unchanged. `get` can find nothing, so it is a `proc`
offering `found` and `missing`, as `list::nth` does. The map itself has no
menu: supplying a key and receiving a value is a function, and `Stream` and
`Seq` remain the negative sequences. `dual(Map<K, V>)` is a consumer of that
map, not a map being built. [`examples/basics/maps.sl`](../../examples/basics/maps.sl)
runs it.

`Hash` answers a non-negative `u64`. Equal values hash equal. The mix is
ordinary prelude code: `wrapping_mul` by the bit pattern of
`0x9E3779B97F4A7C15`, then the high bit cleared. A `String` is FNV-1a over
its scalar values — `char_to_code`, folded in with `xor` and `wrapping_mul`
by the FNV prime — and the same clear. The high bit stays clear because
`rem` is the signed remainder of the machine word, so a negative hash would
not be a slot in `0 .. width`. The integer widths, `char`, `Bool`, and
`String` have impls.
[`examples/basics/hash.sl`](../../examples/basics/hash.sl) runs them.

`Array<T>` is an immutable array: a 4-way trie of positive elements. `Array4<T>`
is one branch, one to four slots, every slot occupied. A computed index is a
match scanned from the first arm, so the branch stays four wide. The digits of
an index in base four select the path. `push` and `update` answer a new array
and leave the one they were given unchanged. `get` and `update` can miss, so
each offers that outcome to a continuation, as `list::nth` does.
[`examples/basics/array.sl`](../../examples/basics/array.sl) runs it.

`HashMap<K, V>` is the unordered map. The path is the hash in base four, and
each digit selects a slot of an `Array4`. A bitmap records which slots are
occupied; the children are packed from the left in slot order. Keys that
hash equal share a list. A key needs `Hash` and `Eq`, and equal keys must
hash equal. `Set<K>` is `Map` with nothing stored beside the key, so its keys
come out in order. `HashSet<K>` is `HashMap` in the same way.
[`examples/basics/sets.sl`](../../examples/basics/sets.sl) runs all three.

Each of the four also has a `Builder`, the negative way to assemble one.
It is a menu in the shape of `string::Builder`: `put(key, value)` answers
the next builder, and `finish` answers the collection that builder holds.
The field is that function, so a bare `put` is what a chain flows into.
A map's `put` takes the entry; a set's takes the key. The states are
persistent. Adding returns a new builder, and a shared prefix can diverge.
`builder` starts from nothing.
[`examples/basics/builders.sl`](../../examples/basics/builders.sl) runs them.

String assembly is a library operation, not a new literal or variadic syntax.
`string::new()` returns a `string::Builder`. `append(part)` answers the next
builder, and `finish` answers the accumulated `String`. The field is the
function `(String -> Builder)`, so `<part | builder.append` is the same
demand. `string::push` renders any `Display` value and uses `append`. Each
builder is persistent: its menu arms close over one
accumulated value, so adding returns a new state and a shared prefix can safely
branch. [`examples/programs/string_builder.sl`](../../examples/programs/string_builder.sl)
shows the complete program.

The program's text comes first in the combined source, so its spans and
line numbers are untouched; a diagnostic inside the library names its unit,
`list.sl:53:57`. Only the units a program reaches are loaded: the prelude always, and each
module named by a path or a `use`, with the modules those name in turn — so a
program that touches no module is checked against the prelude alone. `examples/basics/stdlib.sl` draws on the second layer only.

**`Seq<T>` is the one that pays for menus in ordinary code.** `List` is
data and `Stream` is codata that never ends; a `Seq` is a menu whose single
item answers *whether* there is more, so the recursion lives in the codata
and the branching in the data:

```sl
enum Step<+T, E> { Done, Yield(T, Seq<T, ..E>) }
menu Seq<+T, E> / {..E} { next: Step<T, ..E> }
```

It is produced a step at a time and only as far as it is demanded, which is
what neither neighbour can do — so `seq::filter` over an infinite source is a
terminating program as long as something downstream stops asking:

```sl
<1 | stream::count_from | seq::of_stream | s => (odd, s) | seq::filter | s => (s, 4) | seq::take  // [1, 3, 5, 7]
```

Beside it: `seq::of_list`/`seq::to_list` and `seq::of_stream` for the bridges,
`seq::map`, `seq::filter`, `seq::take`, and `seq::take_while`, which cuts a stream
where a value stops passing and therefore answers a `Seq` — the type saying
what the function does. `examples/laziness/seq.sl` runs all of it. A `Seq` carries
the row its steps perform when demanded, `Seq<T, ..E>`, so `seq::map` with an
effectful function answers at once and its effects happen where the steps are
demanded — under whatever handler is around `seq::to_list`. There is no
`impl Display for Seq`, for the reason `Stream` has none: showing one is
`seq::to_list`, or `seq::take` first if it may not end.

**A stdlib helper that takes both values and continuations is a
`proc`.** That is what the declaration square calls the shape, and the
header says it: the value group before the `|`, the menu of exits after. It
could instead be a returning `func` returning `-T` — the same type, since
`A → ⊥` *is* `-A`, and a consumer transformer cannot do it because its one
parameter group *is* its row — but that spelling says the shape only in the
return position, and it makes the caller build the consumer before cutting
into it rather than write the call every other call is written as. Two
combinators had it and are gone: `then(f, k)`, because composing a function
with a continuation is `f | k>`, and `defaulting(fallback, k)`, because a row
slot wants a consumer and `mu String { m => <fallback | k> }` is the
consumer — the combinator only hid the arm. The `<- A` form remains the
natural spelling for a consumer transformer whose inputs are all
continuations.

**Builtins** are what the language cannot express — I/O, arithmetic on
machine numbers, string internals — and they follow the same rule the
language does: **a builtin whose outcome
is a single value is an ordinary function; a builtin whose outcome is not —
it can fail, or find nothing — takes continuations and denotes a command.**
The value arguments come first, then one continuation per outcome, and exactly
one of them is activated.

| Builtin      | Values                                           | Outcomes                                            |
|--------------|--------------------------------------------------|-----------------------------------------------------|
| `parse_int`  | `text: +String`                                  | `ok: -i64`, `invalid: -String`, `overflow: -String` |
| `char_at`    | `text: +String`, `index: +i64`                   | `ok: -char`, `out_of_range: -String`                |
| `find_char`  | `text: +String`, `from: +i64`, `character: +i64` | `found: -i64`, `absent: -String`                    |

Every failure continuation receives a `+String` describing what happened, so
it composes with an error consumer a program already has.

```sl
<"input.json" | fs::read | (
    mu String { source => <source | parse_json | report> }
    & complain
)>
```

A consumer per outcome is what `mu` builds, so an outcome's handler can be
written where it is passed rather than declared elsewhere.

Everything else is a function: `println`, `print`, and `format`; arithmetic and
comparison; `str_len`, `str_concat`, `int_to_str`, `str_eq`, `substring`;
`is_digit`, `is_ws`, `skip_ws`, `skip_digits`.

**Files are the `fs` module's**, not builtins a program has unasked:
`fs::read`, `fs::write`, `fs::open`, `fs::read_line` offer
their outcomes as above, and `fs::close` spends a handle so a later read
through it fails, `fs::exists` answers a `Bool`. Each performs the `Fs`
effect, so the code that calls them runs under a handler: `fs::real` answers
from the disk through the runtime's primitives, `__read_file` and its
siblings, which are what the language cannot express (see "`IO`: the effect
the runtime handles").

A handle is a value of its own base type, `File`, produced only by
`fs::open` — so nothing else closes a file or reads a line. A read after
`fs::close` is a runtime error.

Composing a close onto an exit closes the file on paths routed through
that wrapper. Shadow `exit` where the handle comes into scope:

```sl
let file = mu { k <= <path | fs::open | (k & complain)> };
let exit = mu i32 { status => { <file | fs::close; <status | exit> } };
```

The arm's `exit` is the outer one. A later direct `| exit>` uses the wrapper,
but a consumer created earlier still captures the old exit. Shadowing does
not rewrite closures or captured continuations. Build failure consumers
used after acquisition around the wrapped exit as well;
`examples/programs/file_io.sl` routes its post-acquisition success and failure paths
this way. Unrestricted control supplies no automatic resource guarantee.

A conversion is the prelude's `Into<+U>`. Between the six integer widths,
and between those widths and `f32` and `f64`, and between `f32` and `f64`,
there is one impl for every pair. The expected type selects the destination.
The number is kept when it fits there; a value that does not fit is an
arithmetic overflow, as `add` overflowing is. There is no truncating or
rounding cast. An integer fits in a float only when that float is exactly
the integer, so an `i64` with no exact `f64` does not fit, while `i64`'s
most negative value does: it is a power of two. A float fits in an integer
width only when it is finite, integral, and inside that width; `NaN`, an
infinity, and a fraction do not. A float fits in `f32` only when it is an
exact `f32`, and every `f32` fits in `f64`. Every integer is one signed
word, so a `u64` reaches as far as `i64` does.
[`examples/basics/into.sl`](../../examples/basics/into.sl) runs the widths
and a few exact float conversions.

`sqrt`, `abs`, `floor`, and `ceil` are unary traits beside `Neg`, so
`<x | sqrt` works for `f32` and `f64`. Square root's domain is the
non-negative reals: `-0.0` is in it, and a negative number is an arithmetic
overflow, `sqrt(-1)`. Absolute value, floor, and ceiling are the IEEE
operations, including `NaN` and the infinities. `f32` arithmetic is IEEE on
the `f64` word both widths share. `num::abs` stays the `i64` function.
Printing a real as an integer is `Into` after `floor`.
[`examples/basics/reals.sl`](../../examples/basics/reals.sl) runs them.
`sin`, `ln`, and `pow` are not in the prelude.

`list::range(from, to)` is the inclusive `i64` list, empty when `from` is
greater than `to`. The one-element case does not add, so `i64`'s greatest
value is a range of itself. `filter` and `fold` are eager and forward the
row their function performs. `fold` takes the list, the initial value, and
a function of `(accumulator, element)`. `sum` is `fold` of addition from
`0`. `reverse` reverses. `take` and `drop` treat a non-positive count as
nothing and the whole list, and a count past the end as the whole list and
nothing. `seq` keeps its own `filter` and `take`. `stream` keeps its own
`take` and `drop`. [`examples/basics/list_ops.sl`](../../examples/basics/list_ops.sl)
runs the eager functions. The quicksort, fannkuch, and sieve benchmarks use
them.

`args::arguments` answers `List<String>`, the words after the source file,
in order. The runtime binary, the file path, `--fuel`, and `--interpret`
are not among them. An empty run is `Nil`, which displays `[]`. Reading
them performs `Args`. `clock::now` answers an `i64` count of monotonic
nanoseconds. The origin is arbitrary and local to the process; the
difference of two readings is the elapsed time, and the count is not a wall
clock. Reading it performs `Clock`. Both are answered the way `Fs` is:
`do expr args::real` and `do expr clock::real` are hands that perform `IO`
and call the runtime. `main` still leaves only `IO`. A count that does not
fit in `i64` is an arithmetic overflow. Out-of-range `__argument_at` is a
fatal type mismatch, as `index` is.
[`examples/basics/arguments.sl`](../../examples/basics/arguments.sl) and
[`examples/basics/clock.sl`](../../examples/basics/clock.sl) run them.
`slc run [--fuel N] [--interpret] <file.sl> [arg]…` is the command; flags
are recognized only before the file. `slc compile [-o <file>] [--fuel N] <file.sl>`
writes the linked executable on x86-64 Linux and reports that it cannot link
on any other host. The executable's arguments are those words, with no fuel
word in front of them. `--fuel` on `compile` is the executable's bound.

Two failures stay fatal rather than becoming outcomes: an out-of-range
`<(s, i) | index` and a division by zero. `index` and `div` are plain
functions, which have nowhere to put a continuation, and — as in Rust, where
`v[i]` panics while `v.get(i)` does not — they report a bug in the program
rather than a case it was meant to handle. The checked forms are the
`proc`-shaped builtins above, `char_at` among them.

A helper of your own that always ends in a cut is annotated `-> (;)`: it never
returns, so it may stand where a consumer is expected.
