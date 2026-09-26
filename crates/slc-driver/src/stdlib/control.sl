pub hook Shift<+A, +R, E> {
    func shift(callback: Delayed<((A -> R / {..E}) -> R / {..E}), ..E>) -> A;
}

pub func reset<+A, +R, E>(program: ((,) -> R / {Shift<A, R, ..E>, ..E})) -> R / {..E} {
    do (<(,) | program) {
        shift(callback): resume => <resume | callback,
    }
}
