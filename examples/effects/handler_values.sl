hook Reader { func config() -> i64; }
hook Offset { func offset() -> i64; }

func scaled(value: i64) -> i64 / {Reader} { <(value, config()) | mul }

proc main | (exit: i32) / {IO} {
    let first = hn Reader { config(): resume => <10 | resume };
    let second = hn [Reader] { config(): resume => <20 | resume };
    let choices = list::List::Cons(first, list::List::Cons(second, list::List::Nil));
    let chosen = <(choices, 1)
        | list::nth
        | (fn(value: (i64 hn i64 / {Reader})) { value } & fn(reason: String) { first });
    <(do (<7 | scaled) chosen) | println;

    let extra = hn Offset { offset(): resume => <2 | resume };
    <(do (do <(<7 | scaled, offset()) | add extra) first) | println;

    let text: (i64 hn String / {Reader}) = hn Reader {
        config(): resume => <10 | resume,
        return(value) => <value | to_string,
    };
    <(do 42 text) | println;
    <0 | exit>
}
