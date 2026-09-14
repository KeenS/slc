// `IO`, the effect the runtime handles.
//
// Reaching outside the program is an effect like any other. The prelude
// declares it —
//
//     effect IO {
//         fn write(text: String) -> (,);
//         fn write_line(text: String) -> (,);
//     }
//
// — and `println` and `print` are the friendly front: they render a value
// through `Display` and then *perform* `write_line`/`write` with the text. So a function that
// prints says so in its row, and the row travels up the call graph the way
// every other effect's does, until something handles it.
//
// What is special about `IO` is only where it ends: the runtime installs a
// handler around `main`, so `main` may declare `{IO}` and leave it
// undischarged. Nothing else may — `main` is the root, and the runtime is
// the one handler it did not have to write.

fn greet(name: String) -> (,) / {IO} {
    <("hello, ", name) | add | println
}

// A pure function stays pure, and the checker holds it to that: printing
// inside this one would be an error rather than a surprise.
fn shout(name: String) -> String {
    <(name, "!") | add
}

command main | (exit: i32) / {IO} {
    // Performed, and unhandled here: it reaches the runtime, which writes.
    <"world" | greet;

    // A handler the program installs sits *nearer* the operation than the
    // runtime's, so it answers first — and the text goes nowhere near the
    // terminal. This is how a program mocks its own output.
    let captured = handle <(<"slant" | shout) | greet {
        write_line(text): resume => text,
        return(u) => "nothing was written",
    };

    // The clause above never resumed, so `greet` stopped where it performed
    // and the handler's value is the text it would have written.
    <("captured instead: ", captured) | add | println;

    // Resuming makes the handler a tap rather than a trap. The clause runs
    // *below* its own prompt, so what it performs escapes outward to the
    // next handler — the runtime's — which is how it both reports the write
    // and forwards it.
    handle <"again" | greet {
        write_line(text): resume => {
            <("about to write ", (<text | str_len | to_string)) | add | x => (x, " characters") | add | println;
            <text | write_line;
            <(,) | resume
        },
    };

    <0 | exit>
}
