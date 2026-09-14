// Input and output through continuations.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: every operation takes one continuation per outcome and activates
// exactly one — the outcome type is the row itself.
//
// Files are the `fs` module's. A file can be read whole with `fs::read`,
// or through a *file*: `fs::open` offers the file or a failure,
// `fs::read_line` offers the next line or the end of the file, and
// `fs::close` spends the file. A file is a value of its own type, `File` — an integer cannot close
// a file, and reading through a closed file fails.
//
// Once a file is open, no path may leave it behind. That is not discipline
// at every cut — it is composition at the only door out: shadow `exit` with
// a consumer that closes the file and then leaves, and every later
// `| exit>` goes through the close, unhappy paths included.

command main | (exit: i32) / {IO} {
    let complain = select String {
        message => {
            <("cannot read: ", message) | add | println;
            <1 | exit>
        },
    };

    // Whole-file reading. `k` is the continuation of the `let`, captured by
    // `mu` — the language's `call/cc` — so the program stays flat.
    let source = mu { k <= <"examples/hello.sl" | fs::read | (k & complain)> };
    <source | print;

    // Line reading, through a file.
    let file = mu { k <= <"examples/hello.sl" | fs::open | (k & complain)> };

    // From here on, `exit` *is* "close the file, then leave": the arm's
    // `exit` is the outer one, and everything below sees only the composed
    // door. No path past this line can end the program with the file open.
    let exit = select i32 {
        status => {
            <file | fs::close;
            <status | exit>
        },
    };

    let first = mu { k <=
        <file | fs::read_line | (k & select unit { end => { <"empty file" | println; <1 | exit> } })>
    };
    <("first line: ", first) | add | println;

    // The failure path: exactly one of the two consumers runs, and this
    // file does not exist. Both consumers leave through the composed exit,
    // so the open file above is closed on these paths too.
    <"examples/missing.sl" | fs::open | (select File {
            unexpected => {
                <"unexpectedly opened" | println;
                <1 | exit>
            },
        } & select String {
            message => {
                <("cannot open: ", message) | add | println;
                <0 | exit>
            },
        })>
}
