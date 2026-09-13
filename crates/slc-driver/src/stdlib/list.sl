// `list`: a list is an ordinary recursive enum — nothing about it is built in.
//
// `use list::List::*;` brings `Nil` and `Cons` in bare; the functions are
// reached as `list::length`, or imported one by one.

mod list {
    pub enum List<+T> {
        Nil,
        Cons(T, List<T>),
    }

    // The import pins `Nil` and `Cons` to List *within this unit*: imports
    // are scoped to their source unit, so a program's own `Nil` — or its own
    // glob — never changes what these mean, and theirs is untouched by ours.
    use List::*;

    pub fn length<+T>(xs: List<T>) -> i64 {
        match xs {
            Nil => 0,
            Cons(_, rest) => (<(1, (<rest | length)) | add),
        }
    }

    pub fn append<+T>(xs: List<T>, ys: List<T>) -> List<T> {
        match xs {
            Nil => ys,
            Cons(h, rest) => Cons(h, <(rest, ys) | append),
        }
    }

    pub fn map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E} {
        match xs {
            Nil => Nil,
            Cons(h, rest) => Cons(<h | f, <(f, rest) | map),
        }
    }

    // Indexing can find nothing, so it offers its outcomes to continuations,
    // the way the lookup builtins do.
    pub command nth<+T>(xs: List<T>, i: i64) | (found: T & missing: String) {
        match xs {
            Nil => <"nothing at that index" | missing>,
            Cons(h, rest) => {
                match (<(i, 0) | eq) { True => { <h | found> }, _ => { <(rest, (<(i, 1) | sub)) | nth | (found & missing)> } }
            },
        }
    }

    fn fmt_items<+T: Display>(xs: List<T>) -> String {
        match xs {
            Nil => "",
            Cons(h, Nil) => <h | fmt,
            Cons(h, rest) => (<((<h | fmt), ", ") | add | x => (x, (<rest | fmt_items)) | add),
        }
    }

    impl<+T: Display> Display for List<T> {
        fn fmt(self: List<T>) -> String { (<("[", (<self | fmt_items)) | add | x => (x, "]") | add) }
    }
}
