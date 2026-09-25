// The simplest SLC program.

command main | (exit: i32) / {IO} {
    <"Hello, SLC!" | println;
    <0 | exit>
}
