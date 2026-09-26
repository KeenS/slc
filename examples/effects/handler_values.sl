hook Reader { func config() -> i64; }
hook Offset { func offset() -> i64; }

func scaled(value: i64) -> i64 / {Reader} { <(value, config()) | mul }

proc main | (exit: i32) / {IO} {
    let first = op Reader { config(): resume => <10 | resume };
    let second = op [Reader] { config(): resume => <20 | resume };
    let choices = list::List::Cons(first, list::List::Cons(second, list::List::Nil));
    let chosen = <(choices, 1)
        | list::nth
        | (fn(value: Handler<i64, i64, {Reader}, {}>) { value } & fn(reason: String) { first });
    <(op chosen do (<7 | scaled)) | println;

    let extra = op Offset { offset(): resume => <2 | resume };
    <(op first do (op extra do <(<7 | scaled, offset()) | add)) | println;

    let text: Handler<i64, String, {Reader}, {}> = op Reader {
        config(): resume => <10 | resume,
        return(value) => <value | to_string,
    };
    <(op text do 42) | println;
    <0 | exit>
}
