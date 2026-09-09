// Input and output through continuations.
//
// Reading a file has two outcomes, so `read_file` does not return one: it
// takes the continuation each outcome belongs to and activates exactly one.
//
// Writing the success continuation inline nests the rest of the program
// inside the call. A local `mu` avoids that: it captures the continuation of
// the expression it stands in — the language's `call/cc` — so the call can
// hand that continuation to `read_file` and the program continues flat.

mu main() | (exit: -i32) {
    // A lambda whose body ends in a cut is a consumer: `+String -> ⊥` is
    // `-String`, which is why this may be passed where one is expected.
    let complain = fn(message: +String) -> ⊥ {
        println("cannot read: " + message);
        1 @ exit
    };

    // `k` is the continuation of this `let`: whatever `read_file` sends it
    // becomes `source`, and the rest of the block runs.
    let source = mu here() | (k: -String) {
        read_file("examples/hello.sl", k, complain)
    };
    print(source);

    // On the failure path the captured continuation is never activated, so
    // nothing below this line runs.
    let missing = mu here() | (k: -String) {
        read_file("examples/missing.sl", k, done)
    };
    println("unexpectedly read " + missing);
    1 @ exit
}

// The failure this example expects: report it and finish successfully.
fn done(message: +String) -> ⊥ {
    println("cannot read: " + message);
    0 @ EXIT
}
