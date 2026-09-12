// Input and output through continuations.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: every operation takes one continuation per outcome and activates
// exactly one — the outcome type is the row itself.
//
// A file can be read whole with `read_file`, or through a *file*:
// `open_file` offers the file or a failure, `read_line` offers the next
// line or the end of the file, and `close_file` spends the file. A file
// is a value of its own type, `+File` — an integer cannot close a file, and
// reading through a closed file fails.
//
// Once a file is open, no path may leave it behind. That is not discipline
// at every cut — it is composition at the only door out: shadow `exit` with
// a consumer that closes the file and then leaves, and every later
// `@ exit` goes through the close, unhappy paths included.

command main | (exit: -i32) {
    let complain = select +String {
        message => {
            println("cannot read: " + message);
            ⟨1 | exit⟩
        },
    };

    // Whole-file reading. `k` is the continuation of the `let`, captured by
    // `mu` — the language's `call/cc` — so the program stays flat.
    let source = mu { k <= read_file("examples/hello.sl", k, complain) };
    print(source);

    // Line reading, through a file.
    let file = mu { k <= open_file("examples/hello.sl", k, complain) };

    // From here on, `exit` *is* "close the file, then leave": the arm's
    // `exit` is the outer one, and everything below sees only the composed
    // door. No path past this line can end the program with the file open.
    let exit = select +i32 {
        status => {
            close_file(file);
            ⟨status | exit⟩
        },
    };

    let first = mu { k <= {
        read_line(file, k, select +unit { end => { println("empty file"); ⟨1 | exit⟩ } })
    } };
    println("first line: " + first);

    // The failure path: exactly one of the two consumers runs, and this
    // file does not exist. Both consumers leave through the composed exit,
    // so the open file above is closed on these paths too.
    open_file(
        "examples/missing.sl",
        select +File {
            unexpected => {
                println("unexpectedly opened");
                ⟨1 | exit⟩
            },
        },
        select +String {
            message => {
                println("cannot open: " + message);
                ⟨0 | exit⟩
            },
        },
    )
}
