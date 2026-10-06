// An anonymous sum names each alternative by position.

func show(x: (i64 | String)) -> String {
    of x {
        ::0(n) => <n | int_to_str,
        ::1(s) => s,
    }
}

proc main | (exit: i32) / {IO} {
    <::0(3) | show | println;
    <::1("hi") | show | println;
    <0 | exit>
}
