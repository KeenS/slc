// Two pure demands. `sequential` forces the left thunk, then the right.
// `parallel` forces both at once. The pairs match. The counted pair reads
// one list.

cite list::List::*;
cite list::length;
cite list::sum;
cite lazy::Lazy;
cite fork::both;
cite fork::sequential;
cite fork::parallel;

func nine() -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <9 | force> }
}

func sixteen() -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <16 | force> }
}

func length_of(xs: list::List<i64>) -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <(<xs | length) | force> }
}

func sum_of(xs: list::List<i64>) -> lazy::Lazy<i64> {
    mu lazy::Lazy { force <= <(<xs | sum) | force> }
}

proc main | (exit: i32) / {IO} {
    let numbers = do (<(nine(), sixteen()) | fork::both) fork::sequential;
    <numbers | println;
    let numbers = do (<(nine(), sixteen()) | fork::both) fork::parallel;
    <numbers | println;
    let xs = Cons(3, Cons(14, Cons(25, Nil)));
    let counted = do (<((<xs | length_of), (<xs | sum_of)) | fork::both) fork::sequential;
    <counted | println;
    let counted = do (<((<xs | length_of), (<xs | sum_of)) | fork::both) fork::parallel;
    <counted | println;
    <0 | exit>
}
