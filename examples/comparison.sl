// Comparisons and boolean operators.

command main | (exit: -i32) {
    println(1 == 1);
    println(1 != 2);
    println(3 < 5);
    println(10 >= 10);
    println('a' < 'b');
    println(true && !false);
    0 | exit
}
