// Nested builtin calls: inner applications lower to mu-bound cuts,
// so composition works without special-casing.

fn main() -> i32 {
    str_concat(int_to_str(add(1, 2)), int_to_str(mul(4, 5)))
}
