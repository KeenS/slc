// mu captures the current continuation; activating it escapes with a value.

command main | (exit: -i32) {
    println(mu escape(k: -i32) {
        42 @ k
    });
    0 @ exit
}
