// `option`: a value that may be absent.
//
// Either/or outcomes are *additive* — one variant, not every field — so
// they are enums, and their consumers are `select`s over them. (A `form`
// would be the wrong connective: it wants every field at once.)

mod option {
    pub enum Option<T> {
        None,
        Some(T),
    }

    pub fn unwrap_or<T>(o: Option<T>, fallback: T) -> T {
        match o {
            Option::None => fallback,
            Option::Some(x) => x,
        }
    }
}
