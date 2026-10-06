# Prelude

The prelude is
[`crates/slc-driver/src/prelude.sl`](https://github.com/KeenS/slc/blob/master/crates/slc-driver/src/prelude.sl).
It is ordinary SLC, appended to every program. A program's own declaration
of a prelude name shadows it. Names from the standard library are not in the
prelude; they are cited.

## Bool, printing, and IO

```sl
enum Bool { False, True }

hook IO {
    func write(text: String) -> (,);
    func write_line(text: String) -> (,);
}

func println<+T: Display>(x: T) -> (,) / {IO}
func print<+T: Display>(x: T) -> (,) / {IO}
func to_string<+T: Display>(x: T) -> String
func not(b: Bool) -> Bool
```

`println` and `print` render through `Display` and then perform `write_line`
or `write`. `not` exchanges `True` and `False`.

## Display

```sl
spec Display {
    func fmt(self: Self) -> String;
}
```

Impls cover `i8`, `i32`, `i64`, `u8`, `u32`, `u64`, `f32`, `f64`, `String`,
`Bool`, `char`, `File`, the unit, and tuples and sums of up to eight
components. A string's `fmt` is the string. `Bool` renders `true` and
`false`. Sums render as `::0(…)` through `::7(…)`.

## Arithmetic and order

```sl
spec Add { func add(self: Self, other: Self) -> Self; }
spec Sub { func sub(self: Self, other: Self) -> Self; }
spec Mul { func mul(self: Self, other: Self) -> Self; }
spec Div { func div(self: Self, other: Self) -> Self; }
spec Rem { func rem(self: Self, other: Self) -> Self; }
spec Neg { func neg(self: Self) -> Self; }
spec Eq {
    func eq(self: Self, other: Self) -> Bool;
    func ne(self: Self, other: Self) -> Bool { <(<(self, other) | eq) | not }
}
spec Ord {
    func lt(self: Self, other: Self) -> Bool;
    func gt(self: Self, other: Self) -> Bool;
    func le(self: Self, other: Self) -> Bool;
    func ge(self: Self, other: Self) -> Bool;
}
```

`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, and `Ord` are implemented for the
six integer widths and for `f32` and `f64`. `Add` is also implemented for
`String`. `Neg` is implemented for the signed widths and the floats. `Eq`
and `Ord` are also implemented for `char`, `String`, and `Bool`. `Ord` and
`Eq` are separate specs.

`Div` and `Rem` by zero are runtime errors, as is an overflowing `Add`,
`Sub`, `Mul`, or `Neg`. The call is `<(a, b) | add`.

```sl
func index(s: String, i: i64) -> char
func wrapping_mul(a: i64, b: i64) -> i64
func xor(a: i64, b: i64) -> i64
```

`index` is the character at a zero-based position. An index outside the
string is a runtime error. `wrapping_mul` is the product on the 64-bit
pattern, and `xor` is exclusive or on that word.

## Into

```sl
spec Into<+U> {
    func into(self: Self) -> U;
}
```

There is an impl for every pair among `i8`, `i32`, `i64`, `u8`, `u32`,
`u64`, `f32`, and `f64`, and none from a type to itself. The expected type
chooses the destination.

The number is kept when it is exact for the destination. Otherwise the
conversion is an arithmetic overflow. An integer fits in a float only when
the float is exactly that integer. A float fits in an integer width only
when it is finite, integral, and inside that width. A float fits in `f32`
only when it is an exact `f32`. Every `f32` fits in `f64`. A negative value
does not fit an unsigned width. A `u64` reaches as far as `i64`, because
every integer is one signed word.

## Float operations

```sl
spec Sqrt  { func sqrt(self: Self) -> Self; }
spec Abs   { func abs(self: Self) -> Self; }
spec Floor { func floor(self: Self) -> Self; }
spec Ceil  { func ceil(self: Self) -> Self; }
```

Each is implemented for `f32` and `f64`. `sqrt` of a negative number is an
arithmetic overflow. `-0.0` is in the domain. `abs`, `floor`, and `ceil` are
the IEEE operations. `num::abs` is the `i64` function in the library, and it
is a different operation.

## Hash

```sl
spec Hash {
    func hash(self: Self) -> u64;
}
```

Impls cover the integer widths, `char`, `Bool`, and `String`. The result is
non-negative. Equal values hash equal. The mix multiplies by the bit pattern
of `0x9E3779B97F4A7C15` and clears the high bit, so a remainder of a hash is
a slot. A `String` is FNV-1a over `char_to_code`.
