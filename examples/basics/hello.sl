// The simplest SLC program.

proc main | (exit: i32) / {IO} {
    <"Hello, SLC!" | println;
    <0 | exit>
}
