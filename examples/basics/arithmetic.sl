// Arithmetic with surface operators.
//
// Division by zero and integer overflow are runtime diagnostics:
//
//   1 / 0    → division by zero
//   9223372036854775807 + 1 → arithmetic overflow

proc main | (exit: i32) / {IO} {
    <(2, 3) | add | println;
    <(10, 4) | sub | println;
    <(6, 7) | mul | println;
    <(100, 10) | div | println;
    <(-7, 2) | add | println;
    <0 | exit>
}
