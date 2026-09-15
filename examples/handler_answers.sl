effect Maths {
    fn seed() -> i64;
    fn combine(left: i64, right: i64) -> i64;
}

fn calculate() -> i64 / {Maths} {
    let initial = seed();
    <(initial, 2) | combine
}

command main | (exit: -i32) / {IO} {
    let answer = handle calculate() {
        seed(): resume => <("resumed: ", <40 | resume) | add,
        combine(left, right): resume => <(left, right) | add | resume,
        return(value) => <("answer: ", <value | int_to_str) | add,
    };
    <answer | println;
    <0 | exit>
}
