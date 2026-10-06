// A form wants every field. An arm that performs IO belongs to the form's row.

form Report / {IO} {
    value: i64,
    label: String,
}

func printer(out: -i64) -> Report {
    mu Report {
        Report { value, label } => {
            <label | println;
            <value | out>
        },
    }
}

func shouting(next: Report) -> Report {
    mu Report {
        Report { value, label } => <Report { value: value, label: <(label, "!") | add } | next>,
    }
}

proc main | (exit: i32) / {IO} {
    <mu i64 { a <= <Report { value: 42, label: "answer" } | (<a | printer)> } | println;
    <mu i64 {
        a <= <Report { value: 7, label: "relabelled" } | (<a | printer | shouting)>,
    } | println;
    <0 | exit>
}
