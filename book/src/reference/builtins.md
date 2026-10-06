# Builtins

A builtin is an operation the surface does not express: machine arithmetic
under the traits, string internals, and the runtime primitives under `fs`,
`args`, and `clock`. The names below are the ones a program calls. The `__`
names are what the prelude and the library call, and a program reaches that
behavior through the trait or the library function.

A builtin with one outcome is a function. A builtin with several outcomes is
a command: the values come first, then one continuation per outcome, and
exactly one continuation runs.

## Strings

| Operation | Arguments | Result |
|---|---|---|
| `str_len` | `String` | `i64`, the number of characters |
| `str_concat` | `String`, `String` | `String` |
| `substring` | `String`, start `i64`, end `i64` | the characters from start up to end |
| `index` | `String`, `i64` | `char`. This is the prelude function over `__index` |
| `int_to_str` | `i64` | `String` |
| `str_eq` | `String`, `String` | `Bool` |
| `char_to_code` | `char` | `i64` |
| `is_digit` | `char` | `Bool` |
| `is_ws` | `char` | `Bool` |
| `skip_ws` | `String`, `i64` | the index after the whitespace |
| `skip_digits` | `String`, `i64` | the index after the digits |

`substring` refuses a range outside the string. `index` refuses an index
outside the string. `is_ws` is true when the character is Unicode
whitespace. `skip_ws` advances over space, tab, newline, and carriage
return. `Add` for `String` is concatenation, and `str_concat` is that
operation under its own name.

## Commands

| Operation | Values | Continuations |
|---|---|---|
| `parse_int` | text `String` | `ok` of `i64`, `invalid` of `String`, `overflow` of `String` |
| `char_at` | text `String`, index `i64` | `ok` of `char`, `out_of_range` of `String` |
| `find_char` | text `String`, from `i64`, character code `i64` | `found` of `i64`, `absent` of `String` |

`index` is the function that fails as a runtime error. `char_at` is the
command that offers the miss to a continuation. `find_char` searches from
the given index for the scalar whose code is the third argument.

## Under the library

These perform `IO` and are reached through a `hand`, not called at the top
of a program that wants its effects in `Fs`, `Args`, or `Clock`.

| Operation | Role |
|---|---|
| `__read_file` | `fs::read`, outcomes text or why |
| `__write_file` | `fs::write` |
| `__open_file` | `fs::open`, a `File` or why |
| `__read_line` | `fs::read_line`, a line or the end |
| `__close_file` | `fs::close` |
| `__file_exists` | `fs::exists` |
| `__argument_count`, `__argument_at` | `args::arguments` |
| `__monotonic_ns` | `clock::now` |

The arithmetic builtins `__add`, `__sub`, `__mul`, `__div`, `__rem`,
`__neg`, the comparisons, `__sqrt`, `__abs`, `__floor`, `__ceil`, and the
`__to_*` conversions are the bodies of the prelude traits.
