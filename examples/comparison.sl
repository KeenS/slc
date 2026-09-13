// Comparisons and boolean operators.

command main | (exit: i32) / {IO} {
    ⟨1 == 1 | println;
    ⟨1 != 2 | println;
    ⟨3 < 5 | println;
    ⟨10 >= 10 | println;
    ⟨'a' < 'b' | println;
    ⟨match true { true => ⟨false | not, _ => false } | println;
    ⟨0 | exit⟩
}
