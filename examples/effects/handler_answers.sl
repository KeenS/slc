hook Maths {
    func seed() -> i64;
    func combine(left: i64, right: i64) -> i64;
}

func calculate() -> i64 / {Maths} {
    let initial = seed();
    <(initial, 2) | combine
}

proc main | (exit: -i32) / {IO} {
    let answer = do calculate() hn {
        seed(): resume => <("resumed: ", <40 | resume) | add,
        combine(left, right): resume => <(left, right) | add | resume,
        return(value) => <("answer: ", <value | int_to_str) | add,
    };
    <answer | println;
    <0 | exit>
}
