command main | (exit: i32) / {IO} {
    let result = <fn {
        let value = <fn(resume: (i64 -> i64)) {
            <(<1 | resume, <2 | resume) | add
        } | control::shift;
        <(value, 10) | mul
    } | control::reset;
    <result | println;
    <0 | exit>
}
