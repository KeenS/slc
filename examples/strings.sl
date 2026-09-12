// String concatenation, indexing, and slicing.

command main | (exit: i32) / {IO} {
    let text: String = "Hello, " + "world!";
    text | println;
    text[0] | println;
    text[7..12] | println;
    0 | exit⟩
}
