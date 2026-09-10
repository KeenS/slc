# Traits and effects: worked sketches

Neither feature is implemented — both are queued in `PLAN.md`. These are
**sketches of the planned surface**, not runnable programs: they will not
parse today, and they live here rather than in `examples/` for exactly that
reason. They exist to pin down how the decided syntax reads and what each
construct elaborates to, so the plan can be judged against real code.

Syntax is as `PLAN.md` settles it: methods are free functions overloaded on a
type (never `x.method()`), bounds are `<T: Trait>`, effect rows ride the
arrow as `/ {E}`, and a handler names its operations and a `resume`.

---

## Traits

### A trait, its impls, and a bounded generic

```sl
trait Show {
    fn show(self: +Self) -> String;
}

impl Show for i64 {
    fn show(self: +i64) -> String { int_to_str(self) }
}

impl Show for bool {
    fn show(self: +bool) -> String {
        if self { "true" } else { "false" }
    }
}

// `show` is resolved by the argument's type — `show(7)` finds the i64 impl,
// `show(true)` the bool impl. No receiver: it is the function it looks like.
fn announce<T: Show>(label: +String, x: +T) -> String {
    label + ": " + show(x)
}

command main | (exit: -i32) {
    println(announce("answer", 42));      // "answer: 42"
    println(announce("flag", true));      // "flag: true"
    0 @ exit
}
```

What the elaboration does, in the language's own existing constructs:

```sl
// the trait becomes the dictionary type (a struct of its methods)
struct Show<Self> { show: (+Self -> String) }

// each impl becomes a dictionary value
const Show_i64:  !Show<i64>  = Show { show: fn(self: +i64) { int_to_str(self) } };
const Show_bool: !Show<bool> = Show { show: fn(self: +bool) { … } };

// the bound becomes an (unrestricted) value parameter, the call a projection
fn announce<T>(dict: !Show<T>, label: +String, x: +T) -> String {
    label + ": " + (match dict { Show { show } => show })(x)
}

// and the call site supplies the coherent impl for the inferred T
announce(Show_i64, "answer", 42)
```

The dictionary is `!` — unrestricted — because `announce` may call `show`
any number of times or none. That is the linear-logic reading of a type
class, and it is why a bound is not a linear resource.

### A generic impl, resolved recursively

```sl
impl<T: Show> Show for List<T> {
    fn show(self: +List<T>) -> String {
        match self {
            Nil        => "[]",
            Cons(h, t) => show(h) + " :: " + show(t),   // inner Show<T>, outer Show<List<T>>
        }
    }
}

fn describe<T: Show>(xs: +List<T>) -> String { show(xs) }
```

`impl<T: Show> Show for List<T>` elaborates to a dictionary-*building*
function `Show<T> -> Show<List<T>>`: given the element's dictionary it
constructs the list's. Resolving `Show<List<i64>>` runs it on `Show<i64>`,
which is the recursion the plan's step (5) is about.

### A trait whose method talks to continuations

A method need not be a positive function. This is the interaction with the
negative side: the dictionary simply holds a `command` value.

```sl
trait Render {
    // not a value-returning fn — a command that sends its output onward
    command render(self: +Self) | (out: -String);
}

impl Render for i64 {
    command render(self: +i64) | (out: -String) {
        int_to_str(self) @ out
    }
}

// the dictionary holds a consumer-taking closure; calling it is ordinary.
// the *continuation* `out` is linear per call; the dictionary that supplied
// `render` is not.
command show_both<T: Render>(x: +T, y: +T) | (out: -String) {
    render(x, select +String {
        first <= render(y, select +String {
            second <= (first + " " + second) @ out,
        }),
    })
}
```

---

## Effects and handlers

An effect is the negative mirror of a trait: where a trait hands a
computation functions a value *provides*, an effect hands it a way to answer
requests the computation *demands*. An operation captures the continuation up
to its handler; `resume` is that continuation, and a handler may invoke it
zero times, once, or many.

### Exceptions — `resume` zero times

```sl
effect Exn {
    throw(msg: +String) -> never;     // never returns to the caller
}

fn checked_div(a: +i64, b: +i64) -> i64 / {Exn} {
    if b == 0 {
        throw("division by zero")     // performs Exn; control leaves for the handler
    } else {
        a / b
    }
}

command main | (exit: -i32) {
    let result = handle checked_div(10, 0) {
        // the handler ignores `resume`: the computation is abandoned, and the
        // handler's value replaces it. dropping `resume` is an exception.
        throw(msg) resume => "error: " + msg,
        return(n)        => "ok: " + int_to_str(n),
    };
    println(result);                  // "error: division by zero"
    0 @ exit
}
```

### Nondeterminism — `resume` twice

This is the showcase: the handler resumes the *same* captured continuation
more than once, which is exactly what the resumable machine enables and what
a language without reifiable continuations cannot express.

```sl
effect Choose {
    choose() -> bool;
}

fn pair() -> String / {Choose} {
    let a = if choose() { "H" } else { "T" };
    let b = if choose() { "H" } else { "T" };
    a + b
}

command main | (exit: -i32) {
    // run `pair` under a handler that takes *both* branches of every choose,
    // by resuming twice and concatenating. The continuation from the first
    // `choose` is entered with `true` and again with `false`.
    let all = handle pair() {
        choose() resume => resume(true) + " " + resume(false),
        return(s)       => s,
    };
    println(all);                     // "HH HT TH TT"
    0 @ exit
}
```

### Why this is not the `Request`-enum pattern

`examples/connectives.sl` already writes codata as a sum of requests a
provider answers:

```sl
enum Request { Retries(↓-i64), Name(↓-String) }
```

That is an effect signature by hand — but the provider answers each request
in tail position and cannot capture the caller's continuation, so it cannot
resume it twice, or abandon it, or resume it later. Effects are this pattern
plus the delimited `resume`. The handler, not the caller, decides what
happens to the rest of the computation.

### The one-shot / multi-shot distinction

A handler that resumes at most once (`Exn`, state, most handlers) is
one-shot; `Choose` is multi-shot. The plan puts this in the effect's type: a
multi-shot handler requires its `resume` be unrestricted, which is the
delicate soundness point, because a one-shot `resume` is the linear
continuation the rest of the language already knows.
