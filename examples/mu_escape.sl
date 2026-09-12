// mu captures the current continuation; activating it escapes with a value.

command main | (exit: -i32) {
    (mu i32 { k <= 42 | k⟩ } | println);
    0 | exit⟩
}
