// Traits: ad-hoc polymorphism by dispatch on a value's type.
//
// A `trait` names operations; an `impl` gives them for a type; a bound
// `<T: Show>` lets a generic call them. A method is a free function
// overloaded on its argument — `show(x)`, never `x.show()` — resolved to the
// right impl by the argument's type. A generic function dispatches at the
// value that actually flows in, so `show` on a list recurses into its
// elements, each resolved on its own.

enum IntList {
    Nil,
    Cons(i64, IntList),
}

trait Show {
    fn show(self: +Self) -> String;
}

impl Show for i64 {
    fn show(self: +i64) -> String { int_to_str(self) }
}

impl Show for bool {
    fn show(self: +bool) -> String {
        if self { "true" } else { "false" }
    }
}

// An impl that calls the trait on its elements: `show(h)` dispatches on the
// element, `show(t)` on the tail.
impl Show for IntList {
    fn show(self: +IntList) -> String {
        match self {
            // Qualified: the prelude's List also has Nil and Cons, so the
            // bare names are ambiguous here.
            IntList::Nil => "nil",
            IntList::Cons(h, t) => show(h) + " :: " + show(t),
        }
    }
}

// A bound generic: `T: Show` lets it call `show` on a value whose type is not
// known here, discharged to a real impl at each call.
fn labelled<T: Show>(label: +String, x: +T) -> String {
    label + ": " + show(x)
}

command main | (exit: -i32) {
    // dispatch on the argument's type
    println(show(42));
    println(show(true));

    // the generic, at two types
    println(labelled("int", 7));
    println(labelled("bool", false));

    // recursive dispatch: the list impl calls show on each element
    println(show(IntList::Cons(1, IntList::Cons(2, IntList::Nil))));

    0 | exit
}
