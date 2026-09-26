// `option`: a value that may be absent.
//
// Either/or outcomes are *additive* — one variant, not every field — so
// they are enums, and their consumers are `mu`s over them. (A `form`
// would be the wrong connective: it wants every field at once.)

pub enum Option<+T> {
    None,
    Some(T),
}

pub func unwrap_or<+T>(o: Option<T>, fallback: T) -> T {
    of o {
        Option::None => fallback,
        Option::Some(x) => x,
    }
}
