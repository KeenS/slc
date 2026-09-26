pub hook Shift<+A, +R, E> {
    func shift(callback: (-> ((A -> R / {..E}) -> R / {..E}) / {..E})) -> A;
}

// The answer is the body's, and the callback's effects are accounted at
// each `do expr control::reset`.
pub hand reset {
    shift(callback): resume => <resume | callback,
}
