// Nested calls compose with surface operators.

command main | (exit: -i32) {
    (1 + 2 | int_to_str) + (4 * 5 | int_to_str) | println;
    0 | exit⟩
}
