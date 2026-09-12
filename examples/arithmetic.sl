// Arithmetic with surface operators.
//
// Division by zero and integer overflow are runtime diagnostics:
//
//   1 / 0    → division by zero
//   9223372036854775807 + 1 → arithmetic overflow

command main | (exit: -i32) {
    2 + 3 | println;
    10 - 4 | println;
    6 * 7 | println;
    100 / 10 | println;
    -7 + 2 | println;
    0 | exit⟩
}
