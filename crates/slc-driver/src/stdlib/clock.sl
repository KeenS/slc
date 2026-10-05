// `clock`: a monotonic nanosecond count. The origin is arbitrary and local
// to the process; the difference of two readings is the time between them.
// It is not a wall clock. Reading it performs `Clock`. `do expr clock::real`
// answers from the runtime.

pub hook Clock {
    func monotonic_ns() -> i64;
}

pub func now() -> i64 / {Clock} {
    monotonic_ns()
}

pub hand real / {IO} {
    monotonic_ns(): resume => <(<(,) | __monotonic_ns) | resume,
}
