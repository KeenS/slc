// `form` — the negative multiplicative, the mirror of `data`.
//
// The declaration square is closed by two axes. `enum`/`menu` split on who
// chooses: an enum value is one variant the producer picked, a menu answers
// one item the consumer demanded. `data`/`form` are the multiplicatives,
// where nobody chooses — every field is in play at once:
//
//   enum Colour { Red, Green }                  |  one variant
//   menu Config { retries: i64 }                &  one item
//   data Report { value: i64, label: String }   ,  every field, given
//   form Report { value: i64, label: String }   ;  every field, wanted
//
// A record carries every field; a form wants every field. Its fields name
// what flows *in*, so `form Report` denotes `(-i64 ; -String)`.

form Report { value: i64, label: String }

// `select` builds the form value, exactly as it builds a menu value: the arm
// binds the whole demand — every field at once — and runs a command.
fn printer(out: -i64) -> Report / {IO} {
    select Report {
        Report { value, label } => {
            ⟨label | println;
            ⟨value | out⟩
        },
    }
}

// A form composes like any consumer: this one relabels, then forwards.
fn shouting(next: Report) -> Report {
    select Report {
        Report { value, label } => ⟨Report { value: value, label: (⟨(label, "!") | add) } | next⟩,
    }
}

command main | (exit: i32) / {IO} {
    // For a negative declaration, `select` builds the value and the literal
    // builds the *demand* on it — `.item(k)` for a menu, `Report { … }` for
    // a form. The cut sends the demand to the form.
    (⟨mu i64 { a <= ⟨Report { value: 42, label: "answer" } | (⟨a | printer)⟩ } | println);

    (⟨mu i64 { a <= ⟨Report { value: 7, label: "relabelled" } | (⟨a | printer | shouting)⟩ } | println);

    // What a form cannot do is give up one field: from `(-A ; -B)` there is no
    // `-A` to be had, the way `(A, B)` yields its `A`. Reading `p.x` off a
    // record is fine because the other fields can be discarded; a form would
    // have to invent them. So a form is always fed whole.

    ⟨0 | exit⟩
}
