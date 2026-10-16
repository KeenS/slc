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

// The function comes first, as it does for `list::map`. `None` stays `None`.
pub func map<+A, +B, E>(f: (A -> B / {..E}), o: Option<A>) -> Option<B> / {..E} {
    of o {
        Option::None => Option::None,
        Option::Some(x) => Option::Some(<x | f),
    }
}

pub func and_then<+A, +B, E>(f: (A -> Option<B> / {..E}), o: Option<A>) -> Option<B> / {..E} {
    of o {
        Option::None => Option::None,
        Option::Some(x) => <x | f,
    }
}
