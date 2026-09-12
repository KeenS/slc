// Nested calls compose with surface operators.

command main | (exit: -i32) {
    println(int_to_str(1 + 2) + int_to_str(4 * 5));
    0 | exit
}
