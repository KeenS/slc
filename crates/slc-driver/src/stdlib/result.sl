// `result`: an outcome that is one of two, as data. A `command` with two
// exits says the same thing as control; this is the value form of it.

mod result {
    pub enum Result<T, E> {
        Ok(T),
        Err(E),
    }
}
