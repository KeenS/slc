# Orientations are not converted

`A -> B` and `B <- A` are different types. A chain reads a stage in the
orientation that stage was written with. A value of one spelling is not
accepted where the other is declared: a field, a parameter, a variant, or
a `let` annotation. The other orientation is a second function.

`examples/duality/two_styles.sl` writes one program each way. The checker
does not turn a closure around, and it does not lift such a turn through a
record, a variant, or a recursive type.
