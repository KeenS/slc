# Migration Guide

This guide covers the syntax changes made during the λ̄μμ̃ redesign. Old forms
are rejected by the current compiler; each section shows the unsupported form
and its replacement.

## Function polarity

Every function now requires an arrow.

### Bare `fn`

Unsupported:

```sl
fn add(x: +i32, y: +i32) { x + y }
```

Write:

```sl
fn add(x: +i32, y: +i32) -> i32 { x + y }
```

### `+fn`

The polarity prefix is removed.

Unsupported:

```sl
+fn add(x: +i32, y: +i32) -> i32 { x + y }
```

Write:

```sl
fn add(x: +i32, y: +i32) -> i32 { x + y }
```

### `-fn`

A negative function consumes continuations and produces the continuation
written after the reverse arrow.

Unsupported:

```sl
-fn k(k1: -i32, k2: -i32) -> (i32, i32) { ... }
```

Write:

```sl
fn k(k1: -i32, k2: -i32) <- (i32, i32) { ... }
```

The comma-separated parameters before `<-` form the consumed continuation row.
The type after `<-` is the continuation produced by the function.

## Consumer abstraction

The declaration formerly named `command` is now `mu`.

### Rename `command` to `mu`

Unsupported:

```sl
command route(x: +i32, k: -i32) { k(x) }
```

Write:

```sl
mu route(x: +i32) | (k: -i32) { k(x) }
```

Value parameters and continuation parameters now occupy separate
parenthesized groups.

### Remove `to`

The directional `to` marker is removed.

Unsupported:

```sl
mu route(x: +i32, to k: -i32) { k(x) }
```

Write:

```sl
mu route(x: +i32) | (k: -i32) { k(x) }
```

### Bottom annotation

The declaration denotes a command, so its result is bottom. The annotation is
optional and does not change lowering:

```sl
mu route(x: +i32) | (k: -i32) -> ⊥ { k(x) }
```

## Partial agents

The old partial-agent forms are removed:

```sl
agent.to(k, h)
agent.consume(k, h)
fn.partial(a)
```

There is no accepted replacement. Construct the needed ordinary function or
negative function with explicit parameters instead.

## Removed type formers

### `Command<I, O>`

The `Command<I, O>` surface type former is removed. It was an internal
symmetric-agent convenience and is not part of the accepted λ̄μμ̃ surface.

Use the explicit polarity-bearing type forms instead:

```sl
(-I ⅋ O)
```

For a function-shaped positive type, use:

```sl
fn(I) -> O
```

For a negative function declaration, use:

```sl
fn(k: -O) <- I
```

### Expression-level `dual(e)`

Expression-level `dual(e)` is removed. It denoted the same witness as `e`
and relied on the checker to reinterpret its polarity, so it did not add a
distinct core term. The type-level `dual(A)` form remains available when an
explicit dual type is needed.

## Continuation activation

Activating a continuation is a cut, not a call. A call supplies an argument to
a function and returns; a cut sends a value to a consumer and does not return.

```sl
k(v)        // old
v @ k       // new

EXIT(0)     // old
0 @ EXIT    // new
```

`@` binds more loosely than every operator, so `a + b @ k` sends the sum, and
it is not associative. The consumer may be any expression that produces one,
including a negative function applied to its row: `Color::Blue @ code(answer)`.

Applying a function to a continuation is unchanged: it is ordinary call
syntax, because the callee's declared row already says which arguments are
consumers. `deliver(ok, err)` still reads as it did.

A cut has type `⊥`, so a branch that ends in one leaves the type of an `if` to
the other branch, and code after a cut in a block is unreachable.

## The entry point is a `mu`

A program is a command, so `main` is a `mu` that takes no values and one
continuation — its exit status:

```sl
fn main() -> i32 {          // old
    println("hi");
    42
}

mu main() | (exit: -i32) {  // new
    println("hi");
    0 @ exit
}
```

The final value is no longer printed: a program's output is exactly what it
prints, and its status is what it sends to `exit`. A `main` that used to end
with a value must print it. Inside `main`, cut against the `exit` parameter
rather than the global `EXIT` — a `mu` must consume the continuation it was
given, so a `main` that never reaches `exit` is a linearity error.

## `select` covers any positive type

`select` is no longer restricted to enums, and its arms are now
`command => pattern`, using the same patterns `match` uses:

```sl
select Color {                     // one arm per variant — the negative additive
    0 @ return => Red,
    1 @ return => Green,
}

select Reading {                   // one arm, binding every field — the negative multiplicative
    (int_to_str(value) + unit) @ out => Reading { value, unit },
}

select (+i64 ⊗ +i64) {             // a bare product names its type
    (left + right) @ out => (left, right),
}
```

An arm's right-hand side used to be a variant name with an optional payload
binder; it is now a pattern, so a struct or tuple shape is written the way
`match` writes it. An arm's left-hand side is a command, as before.

## Struct literals and patterns

A struct literal is an ordinary expression — it used to parse only as a call
argument — and a struct pattern now binds its fields at run time, which it
silently failed to do. A struct value is a labelled product, the same shape an
`enum` variant has.

## Fallible builtins

A builtin that can fail no longer aborts the program: it takes the
continuation each outcome belongs to.

```sl
let content = read_file(path);          // old: a runtime error if it fails

read_file(path, fn(content: +String) -> ⊥ {   // new
    ...
}, fn(message: +String) -> ⊥ {
    ...
})
```

The same applies to `write_file`, `char_at`, `list_get`, `map_get`, and
`find_char`. `__parse_int` is renamed `parse_int`, and its failure
continuations now receive a message rather than the original input.
`str_to_int` is removed; use `parse_int`.

An out-of-range `s[i]` and a division by zero stay fatal: they are operator
syntax with nowhere to put a continuation, and they report a bug rather than a
case to handle.

## Values and continuations together

A declaration that takes both values and continuations is a `mu`, not a `fn`:

```sl
// old: a positive function carrying a consumer and returning a position
fn parse_value(input: +String, pos: +i64, report: -ParseResult) -> i64 { ... }

// new: a command with a value group and a continuation group
mu parse_value(input: +String, pos: +i64) | (ok: -i64, report: -ParseResult) { ... }
```

The body of a `mu` ends in a cut rather than returning: what was a returned
position is sent to `ok`. Where the old code sequenced two fallible steps with
`let`, the new code passes the rest of the work as a continuation:

```sl
// old
let value_end = parse_value(input, pos, report);
...

// new
parse_value(input, pos, fn(value_end: +i64) -> ⊥ {
    ...
}, report)
```

A helper that only computes with values stays an ordinary positive `fn`.

## Additive control

### `select`

The old experimental `select` declaration no longer defines a new type.
The final form consumes an existing enum:

```sl
enum Color { Red, Green, Blue }

fn k(return: -i32) <- Color {
    select Color {
        0 @ return => Red,
        1 @ return => Green,
        2 @ return => Blue,
    }
}
```

Arms must cover each enum variant exactly once. An arm binds the payload of
its variant when the variant declares one, and passes it to that arm's
consumer:

```sl
enum ParseResult { Parsed(String), Failed(String) }

fn deliver(ok: -String, err: -String) <- ParseResult {
    select ParseResult {
        text @ ok => Parsed(text),
        message @ err => Failed(message),
    }
}
```

A pair of success and failure continuations threaded through every function
can therefore be replaced by one continuation that accepts a result enum.

### `choose`

Old variant-style `choose T { Variant }` syntax is removed.

Experimental struct-based `choose` implementation work has been removed while
its design is deferred. Do not migrate programs to any `choose` form; a future
design will be discussed separately.

## Removed constructs

### `spawn`

`spawn` is removed entirely and has no replacement:

```sl
spawn { ... }
```

Concurrency-like process creation is not part of the λ̄μμ̃ core.

### Expression-level value-returning `mu`

The old expression-level/value-returning `mu` is removed. Use either a
positive function or the final `mu(values) | (continuations)` declaration,
according to the intended control behavior.
