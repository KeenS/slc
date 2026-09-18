pub effect Shift<+A, +R, E> {
    fn shift(callback: Delayed<((A -> R / {..E}) -> R / {..E}), ..E>) -> A;
}

pub fn reset<+A, +R, E>(program: ((,) -> R / {Shift<A, R, ..E>, ..E})) -> R / {..E} {
    handle (<(,) | program) {
        shift(callback): resume => <resume | callback,
    }
}
