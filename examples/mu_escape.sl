// mu captures the current continuation; activating it escapes with a value.

command main | (exit: -i32) {
    println(mu i32 { k <= ⟨42 | k⟩ });
    ⟨0 | exit⟩
}
