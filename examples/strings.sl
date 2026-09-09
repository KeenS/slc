// String concatenation, indexing, and slicing.

mu main() | (exit: -i32) {
    let text: +String = "Hello, " + "world!";
    println(text);
    println(text[0]);
    println(text[7..12]);
    0 @ exit
}
