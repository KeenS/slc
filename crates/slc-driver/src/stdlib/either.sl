// `either`: a value that is one of two, as data.
//
// Neither side means success: `Left` and `Right` are only which one it is.
// A `command` with two exits says the same thing as control; this is the
// value form of it, for when the choice has to be kept rather than taken.

mod either {
    pub enum Either<+L, +R> {
        Left(L),
        Right(R),
    }
}
