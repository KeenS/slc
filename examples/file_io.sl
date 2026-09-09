// Input and output through continuations.
//
// Reading a file has two outcomes, so `read_file` does not return one: it
// takes the continuation each outcome belongs to and activates exactly one.
//
// Both consumers here are built by `select`, in the two shapes it takes: over
// an atom, whose single arm binds the value that arrives, and over an `enum`,
// whose arms are one per outcome.

command main | (exit: -i32) {
    // `select` over an atom is a consumer literal: the arm names what arrives
    // and runs a command with it.
    let complain = select +String {
        message <= {
            println("cannot read: " + message);
            1 @ exit
        },
    };

    // `k` is the continuation of this `let`: whatever `read_file` sends it
    // becomes `source`, and the rest of the block runs. The local `mu` — the
    // language's `call/cc` — is what keeps the program flat; without it, every
    // line below would nest inside the success consumer.
    //
    // `k` needs no annotation: it is handed to a slot `read_file` declares,
    // which makes it a `-String`, and the `let` a `+String`.
    let source = mu | (k) {
        read_file("examples/hello.sl", k, complain)
    };
    print(source);

    // The same two outcomes, named rather than passed side by side: `read`
    // sends one `Read`, and one `select` over the enum answers both.
    read("examples/missing.sl", select Read {
        Contents(text) <= {
            println("unexpectedly read " + text);
            1 @ exit
        },
        Failed(message) <= {
            println("cannot read: " + message);
            0 @ exit
        },
    })
}

enum Read {
    Contents(String),
    Failed(String),
}

// Two continuations become one send by tagging: each atom consumer labels what
// it receives and forwards it to `out`. Exactly one of them ever runs.
command read(path: +String) | (out: -Read) {
    read_file(
        path,
        select +String { text <= Read::Contents(text) @ out },
        select +String { message <= Read::Failed(message) @ out },
    )
}
