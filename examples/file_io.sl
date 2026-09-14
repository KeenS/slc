// Input and output through continuations.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: every operation takes one continuation per outcome and activates
// exactly one — the outcome type is the row itself.
//
// Files are the `fs` module's. A file can be read whole with `fs::read`,
// or through a *file*: `fs::open` offers the file or a failure,
// `fs::read_line` offers the next line or the end of the file, and
// `fs::close` spends the file. A file is a value of its own type, `File` — an
// integer cannot close a file, and reading through a closed file fails.
//
// Touching a file is the `Fs` effect, and `fs::real` answers it from the
// disk: the program's file work runs under it, and hands back the status the
// program ends with. The same work could run under a handler of the
// program's own, which answers from somewhere else.
//
// Once a file is open, no path may leave it behind. That is not discipline
// at every cut — it is composition at the only door out: shadow `done` with
// a consumer that closes the file and then leaves, and every later
// `| done>` goes through the close, unhappy paths included.

command main | (exit: i32) / {IO} {
    let status = <(fn(u: (,)) {
        mu i32 { done <= {
            let complain = select String {
                message => {
                    <("cannot read: ", message) | add | println;
                    <1 | done>
                },
            };

            // Whole-file reading. `k` is the continuation of the `let`,
            // captured by `mu` — the language's `call/cc` — so the program
            // stays flat.
            let source = mu { k <= <"examples/hello.sl" | fs::read | (k & complain)> };
            <source | print;

            // Line reading, through a file.
            let file = mu { k <= <"examples/hello.sl" | fs::open | (k & complain)> };

            // From here on, `done` *is* "close the file, then leave": the
            // arm's `done` is the outer one, and everything below sees only
            // the composed door. No path past this line can finish with the
            // file open.
            let done = select i32 {
                status => {
                    <file | fs::close;
                    <status | done>
                },
            };

            let first = mu { k <=
                <file | fs::read_line | (k & select unit { end => { <"empty file" | println; <1 | done> } })>
            };
            <("first line: ", first) | add | println;

            // The failure path: exactly one of the two consumers runs, and
            // this file does not exist. Both leave through the composed door,
            // so the open file above is closed on these paths too.
            <"examples/missing.sl" | fs::open | (select File {
                    unexpected => {
                        <"unexpectedly opened" | println;
                        <1 | done>
                    },
                } & select String {
                    message => {
                        <("cannot open: ", message) | add | println;
                        <0 | done>
                    },
                })>
        } }
    }) | fs::real;
    <status | exit>
}
