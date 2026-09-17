// The simplest Slant program.

command main | (exit: i32) / {IO} {
    <"Hello, Slant!" | println;
    <0 | exit>
}
