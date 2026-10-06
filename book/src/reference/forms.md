# Surface forms

These are the shapes a program writes. A `…` stands for a form already in
the list. The parser in `slc` is the grammar; this page is the index.

## Declarations

```sl
func name(param: Type, …) -> Type / {Row} { Expr }
func name<+T: Bound>(param: Type) -> Type { Expr }
func name(param: Type) <- Type / {Row} { Expr }

proc name(param: Type, …) | (exit: Type & …) / {Row} { Expr }

data Name<+T> { field: Type, … }
enum Name<+T> { Variant, Variant(Type, …), … }
menu Name<+T, E> / {..E} { field: Type, field(param: Type): Type, … }
form Name / {Row} { field: Type, … }

spec Name<+U>: Parent + Parent {
    type Item;
    func method(self: Self, …) -> Type;
    func method(self: Self) -> Type { Expr }
}
impl<+T: Bound> Name<Arg> for Type {
    type Item = Type;
    func method(self: Type) -> Type { Expr }
}

hook Name<+T, E> { func operation(param: Type, …) -> Type; }
hand name / {Row} { operation(param): name => Expr, _ => forward, }

def NAME: Type = Expr;
pub Declaration
sect name { Declaration … }
sect name;
cite path::name;
cite path::*;
cite Enum::{Variant, …};
```

## Expressions

```sl
name
literal
(,)
(Expr, Expr, …)
::index(Expr)
Name { field: Expr, … }
Variant(Expr, …)
name.field
name.index

{ Statement; … Expr }
let pattern = Expr
let+ pattern = Expr
let- pattern = Expr

of Expr { pattern => Expr, … }
mu Type { pattern => Expr, … }
mu Type { copattern <= Expr, … }
mu Type { name <= Expr }
mu Type {}

fn(param: Type, …) -> Type { Expr }
fn { Expr }
<(,) | name
name()

do Expr Handler
hn Effect { Clause, … }
hn [Effect, …] { Clause, … }
hn { Clause, … }
reset Expr

<Expr | Stage | … 
<Expr | Stage | … | Consumer>
Stage | Stage
name => Expr
name <= Expr
```

`Handler` is an `hn` expression, a `hand` name, or a name bound to a handler
value. `Stage` is an expression or a binder. A cut includes the closing `>`.

## Types

```sl
i8  i32  i64  u8  u32  u64  f32  f64  char  String  Bool  File  unit
+Type   -Type   *Type   dual(Type)
Type -> Type
Type <- Type
(-> Type / {Row})
(Type hn Type / {Row} / {Row})
(Type, Type, …)     (,)
(Type | Type | …)   (|)
(Type & Type & …)   (&)
(Type ; Type)       (;)
Name<Type, …>
Name::Item<Type, …>
/ {Effect, …, ..Name}
```

The unit of a connective is the parentheses holding only its separator.

## Patterns

```sl
name
_
name @ pattern
literal
start..=end
pattern | pattern
(pattern, pattern, …)
Name { field, field: pattern, … }
Variant
Variant(pattern, …)
Path::Variant(pattern, …)
::index(pattern)
```

## Clauses

```sl
operation(param, …) => Expr
operation(param, …): name => Expr
operation(): name => Expr
return(name) => Expr
_ => forward
```
