// String concatenation, indexing, and slicing.

proc main | (exit: i32) / {IO} {
    let text: String = <("Hello, ", "world!") | add;
    <text | println;
    <(text, 0) | index | println;
    <(text, 7, 12) | substring | println;
    <0 | exit>
}
