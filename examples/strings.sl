// String concatenation, indexing, and slicing.

command main | (exit: -i32) {
    let text: +String = "Hello, " + "world!";
    text | println;
    text[0] | println;
    text[7..12] | println;
    0 | exit⟩
}
