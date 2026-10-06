// The words after the source file. A run with none prints [].

proc main | (exit: i32) / {IO} {
    let words = do (<(,) | args::arguments) args::real;
    <words | println;
    <0 | exit>
}
