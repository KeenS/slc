// The four logical units, supplied by the prelude.
//
// Unit and Bottom name the multiplicative units already written `()` and
// `⊥`. Empty and Top are the nullary additive declarations; concise aliases
// for them remain deliberately unsettled.

fn unit_value() -> Unit {
    ()
}

// Bottom's nullary demand is the unit value itself: dual(⊥) = 1.
fn bottom_demand() -> Unit {
    Bottom {}
}

fn use_empty<T>(empty: Empty) -> T {
    match empty {}
}

fn top_value() -> Top {
    mu Top {}
}

command main | (exit: -i32) -> Bottom {
    println(unit_value());
    println(bottom_demand());
    top_value();
    0 @ exit
}
