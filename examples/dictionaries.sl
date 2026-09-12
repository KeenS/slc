// Traits with no runtime method value: dispatch is resolved at compile time.
//
// A call on a concrete type is a direct call to the impl. A call on a bound
// type parameter `<T: Show>` projects the method from a dictionary the caller
// passes — and a bounded function forwards its dictionary to the next, so the
// impl is chosen once, by whoever knew the concrete type.

trait Show { fn show(self: +Self) -> String; }
impl Show for i64  { fn show(self: +i64)  -> String { int_to_str(self) } }
impl Show for bool { fn show(self: +bool) -> String { if self { "T" } else { "F" } } }

// Polymorphic: `show` here projects from `twice`'s dictionary parameter.
fn twice<T: Show>(x: +T) -> String { show(x) + show(x) }
// Forwards its dictionary one level deeper, into `twice`.
fn relay<T: Show>(x: +T) -> String { "[" + twice(x) + "]" }

command main | (exit: -i32) {
    println(show(42));      // 42  — concrete receiver, a direct impl call
    println(relay(7));      // [77] — i64 dictionary threaded through relay→twice
    println(relay(true));   // [TT] — bool dictionary, same code
    0 | exit
}
