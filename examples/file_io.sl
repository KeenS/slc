// Input and output through continuations.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: every operation takes one continuation per outcome and activates
// exactly one — the outcome type is the row itself.
//
// A file can be read whole with `read_file`, or through a *handle*:
// `open_file` offers the handle or a failure, `read_line` offers the next
// line or the end of the file, and `close_file` spends the handle. A handle
// is a value of its own type, `+File` — an integer cannot close a file, and
// reading through a closed handle fails.

command main | (exit: -i32) {
    let complain = select +String {
        message <= {
            println("cannot read: " + message);
            1 @ exit
        },
    };

    // Whole-file reading. `k` is the continuation of the `let`, captured by
    // `mu` — the language's `call/cc` — so the program stays flat.
    let source = mu(k) {
        read_file("examples/hello.sl", k, complain)
    };
    print(source);

    // Line reading, through a handle.
    let handle = mu(k) {
        open_file("examples/hello.sl", k, complain)
    };
    let first = mu(k) {
        read_line(handle, k, select +unit { end <= { println("empty file"); 1 @ exit } })
    };
    println("first line: " + first);
    close_file(handle);

    // The failure path: exactly one of the two consumers runs, and this
    // file does not exist.
    open_file(
        "examples/missing.sl",
        select +File {
            unexpected <= {
                println("unexpectedly opened");
                1 @ exit
            },
        },
        select +String {
            message <= {
                println("cannot open: " + message);
                0 @ exit
            },
        },
    )
}
