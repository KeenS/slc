# A first program

```sl
{{#include ../../examples/hello.sl}}
```

Run it from the repository:

```sh
slc run book/examples/hello.sl
```

It prints

```text
Hello, SLC!
```

and exits with status 0.

## The entry point

`proc main` is the entry point. It takes no values and one continuation,
`exit`. The runtime supplies that continuation. The integer sent to `exit` is
the process status.

`{IO}` is the effect of printing. The runtime handles it. A `main` that only
sends a status, and prints nothing, can leave the row off. A `func main`, a
`main` with value parameters, and a `main` inside a section are refused. The
diagnostic names the shape `proc main | (exit: -i32) / {IO}`.

Every terminating path of `main` sends a status to `exit`. Falling off the
end is a type error. There is no separate exit value sitting beside `main`:
ending the program is the continuation `main` was given, and a helper receives
it only when `main` passes it on.

## A chain

`<` opens a chain with a value. `|` carries that value to the next stage.
`<"Hello, SLC!" | println` sends the string to `println`. The semicolon ends
the statement, and the block continues. The chain has no closing `>`, so it
is an application: `println` returns the unit value `(,)` after writing the
line.

`<0 | exit>` is a cut. The `>` delivers `0` to the consumer `exit` and does
not come back. A cut is how a command ends.

A chain says what it is at both ends. `<` makes it start from a value, `>`
makes it deliver to a consumer, and the two together are a cut. `"Hello,
SLC!" | println` is refused, because a string is a value and a chain without
`<` starts with a function. `<0` alone is refused, because the value has
nowhere to go.

`println` renders its argument through `Display` and performs `IO`. `print`
is the same operation without a trailing newline.
