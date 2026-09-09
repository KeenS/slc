// Bottom type.
//
// `⊥` is the dual of unit: it is inhabited on the negative side only.
// A continuation of type `-⊥` accepts an impossible value, so once it is
// supplied, the rest of the computation need not return normally.
//
// `absurd` is a negative function: it consumes the bottom continuation `k`
// and produces the continuation that `main` cuts against.

fn absurd(k: -⊥) <- i64 {
    () @ k
}

mu main | (exit: -i32) {
    println(mu halt | (out: -⊥) {
        absurd(out)
    });
    0 @ exit
}
