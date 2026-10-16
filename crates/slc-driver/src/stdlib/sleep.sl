// `sleep`: wait. The duration is a count of nanoseconds, the same unit as
// `clock::now`. Zero returns at once. A negative count is an arithmetic
// overflow. Waiting performs `Sleep`. `do expr sleep::real` answers from
// the runtime.

pub hook Sleep {
    func delay_ns(ns: i64) -> (,);
}

pub func sleep(ns: i64) -> (,) / {Sleep} {
    <ns | delay_ns
}

pub hand real / {IO} {
    delay_ns(ns): resume => <(<ns | __sleep_ns) | resume,
}
