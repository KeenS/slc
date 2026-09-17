effect Reader { fn config() -> i64; }
effect Offset { fn offset() -> i64; }

fn scaled(value: i64) -> i64 / {Reader} { <(value, config()) | mul }

command main | (exit: i32) / {IO} {
    let first = handler Reader { config(): resume => <10 | resume };
    let second = handler [Reader] { config(): resume => <20 | resume };
    let choices = list::List::Cons(first, list::List::Cons(second, list::List::Nil));
    let chosen = <(choices, 1)
        | list::nth
        | (fn(value: Handler<i64, i64, {Reader}, {}>) { value } & fn(reason: String) { first });
    <(with chosen handle (<7 | scaled)) | println;

    let extra = handler Offset { offset(): resume => <2 | resume };
    <(with first handle (with extra handle <(<7 | scaled, offset()) | add)) | println;

    let text: Handler<i64, String, {Reader}, {}> = handler Reader {
        config(): resume => <10 | resume,
        return(value) => <value | to_string,
    };
    <(with text handle 42) | println;
    <0 | exit>
}
