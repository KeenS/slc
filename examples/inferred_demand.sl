effect Build { fn build() -> i64; }

fn ignore(callback: Delayed<(i64 -> i64), {Build}>) -> i64 { 0 }

fn twice(callback: Delayed<(i64 -> i64), {Build}>) -> i64 / {Build} {
    <(<1 | callback, <2 | callback) | add
}

fn answer() -> i64 / {IO} { <"called" | println; 42 }

command main | (exit: i32) / {IO} {
    let discard = fn(value) { <{ build(); value } | ignore };
    <fn(input: i64) { input } | discard | println;

    let repeat = fn(value) {
        let pending = { build(); value };
        handle (<pending | twice) {
            build(): resume => { <"demand" | println; <0 | resume }
        }
    };
    <fn(input: i64) { input } | repeat | println;

    let factory = answer;
    <"stored" | println;
    <(,) | factory | println;
    <answer() | println;
    <0 | exit>
}
