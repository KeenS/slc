// Input and output through continuations.
//
// Reading a file has two outcomes, so `read_file` does not return one: it
// takes the continuation each outcome belongs to and activates exactly one.
//
// One continuation per outcome is the whole outcome type: a consumer of
// `A ⊕ B` is a consumer of `A` and a consumer of `B`, so naming the outcomes
// as an `enum` and sending it to a single continuation would only wrap what
// the row already says. Each consumer here is built by `select` over the type
// it receives.

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
    let source = mu(k) {
        read_file("examples/hello.sl", k, complain)
    };
    print(source);

    // This read fails, so control leaves through the second consumer and
    // nothing after this call runs — the two are alternatives, and exactly
    // one of them is activated.
    read_file(
        "examples/missing.sl",
        select +String {
            text <= {
                println("unexpectedly read " + text);
                1 @ exit
            },
        },
        select +String {
            message <= {
                println("cannot read: " + message);
                0 @ exit
            },
        },
    )
}
