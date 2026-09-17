// Comparisons and boolean operators.

command main | (exit: i32) / {IO} {
    <(1, 1) | eq | println;
    <(1, 2) | ne | println;
    <(3, 5) | lt | println;
    <(10, 10) | ge | println;
    <('a', 'b') | lt | println;
    <match True { True => <False | not, False => False } | println;
    <0 | exit>
}
