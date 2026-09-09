// Nested calls compose with surface operators.

fn main() -> i32 {
    int_to_str(1 + 2) + int_to_str(4 * 5)
}
