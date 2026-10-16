// Strict boolean operations, and shifts of an `i64` bit pattern. The shift
// distance is the count's low six bits. `shr` fills from the left with zeros.

proc main | (exit: i32) / {IO} {
    <(True, False) | and | println;
    <(True, True) | and | println;
    <(False, True) | or | println;
    <(False, False) | or | println;
    <(1, 4) | shl | println;
    <(16, 2) | shr | println;
    <(-1, 1) | shr | println;
    <(1, 63) | shl | println;
    <(1, -1) | shl | println;
    <0 | exit>
}
