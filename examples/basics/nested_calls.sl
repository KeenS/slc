// Nested calls compose with surface operators.

proc main | (exit: i32) / {IO} {
    <(<(1, 2) | add | int_to_str, <(4, 5) | mul | int_to_str) | add | println;
    <0 | exit>
}
