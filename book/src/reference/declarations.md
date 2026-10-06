# Declarations

A file is a sequence of declarations. The order does not decide visibility:
names in one section see each other, and a call may precede the declaration
it names.

`pub` may precede a declaration that sits in a section. Without `pub`, the
declaration is visible to that section and the sections nested inside it.

## func

```sl
func name(param: Type, …) -> Result / {Effects} { body }
func name<+T, …>(param: Type) -> Result { body }
func name(consumer: Type) <- Input / {Effects} { body }
```

The value parameters are one group. `->` returns a value. `<-` produces a
consumer of `Input`; each parameter on a `<-` function is a consumer, and
the positive type written on it is what that consumer accepts. A bare arrow
has an empty effect row.

A type parameter carries a sign: `+` positive, `-` negative, `*` either.
An unsigned parameter is a row, used as `..E`. Bounds are written on the
parameter, `<+T: Eq + Display>`.

## fn

`fn` is an expression, listed here because it is the unnamed form of `func`.

```sl
fn(param: Type, …) -> Result { body }
fn { body }
```

A nullary lambda's name denotes the function. Calling it is `f()` or `<(,) |
f`.

## proc

```sl
proc name(value: Type, …) | (exit: Consumer, …) / {Effects} { body }
```

The body is a command. Every terminating path cuts to one of the exits. The
exit row is a bundle: positional, invariant in width and order. `main` is a
`proc` at the root with no value parameters and one exit, the status.

## data and enum

```sl
data Name<+T> { field: Type, … }
enum Name<+T> { Variant, Variant(Type, …), … }
```

A `data` value carries every field. An `enum` value carries exactly one
variant. A variant with no payload is a value of the enum. A variant with a
payload is a constructor, and several payload types are one tuple. Type
parameters on `data`, `enum`, `menu`, and `form` take signs and do not take
trait bounds.

## menu and form

```sl
menu Name<+T, E> / {..E} {
    item: Type,
    item(param: Type, …): Type,
}

form Name / {Effects} { field: Type, … }
```

A menu answers one item the consumer demands. A field with parameters is the
function of those parameters, and the answer type is that function's result.
A form wants every field at once. Both are built with `mu`. The effect row
on the declaration is performed when the value is demanded.

## spec and impl

```sl
spec Name<+U>: Parent + Parent {
    type Item;
    func method(self: Self, …) -> Result / {Effects};
    func method(self: Self) -> Result { body }
}

impl<+T: Bound> Name<Arg> for Type {
    type Item = Concrete;
    func method(self: Type, …) -> Result { body }
}
```

`Self` is the implementing type. A method body in the spec is a default,
copied into an impl that omits it. See [Traits](traits.md).

## hook, hand, and hn

```sl
hook Name<+T, E> {
    func operation(arg: Type, …) -> Result;
}

hand name / {Residual} {
    operation(arg): resume => body,
    _ => forward,
}
```

A `hook` declares an effect. A `hand` declares a handler and is not a value;
each `do expr name` inlines it. `hn` is an expression that builds a handler
value. See [Effects and handlers](effects.md).

## def

```sl
def NAME: Type = expression;
```

`def` binds a constant. The name sits on neither side of the value and
continuation mirror, which is why the keyword is `def`.

## sect and cite

```sl
sect name { declarations }
sect name;
cite path::name;
cite path::*;
cite Enum::{Variant, Variant};
```

See [Sections and files](sections.md).
