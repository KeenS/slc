// Scans over a string: a prefix, the scalar values, a split, a trim, a
// replacement, and Unicode case mapping.

proc main | (exit: i32) / {IO} {
    <("Hello", "He") | string::starts_with | println;
    <("Hello", "he") | string::starts_with | println;
    <("Hello", "") | string::starts_with | println;
    <"Hello" | string::chars | println;
    <("a,b,c", ",") | string::split | println;
    <"  hi  " | string::trim | println;
    <("a-b-a", "a", "c") | string::replace | println;
    <"AbC" | string::lower | println;
    <"AbC" | string::upper | println;
    <"straße" | string::upper | println;
    <0 | exit>
}
