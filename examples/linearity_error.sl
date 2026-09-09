// This program is intentionally ill-typed: the continuation `k` is
// never activated. Continuations are linear: they must be used
// exactly once on every path.
//
// Run it to see the diagnostic:
//
//   slc run examples/linearity_error.sl

mu bad(x: +i32) | (k: -i32) {
    x
}

mu main() | (exit: -i32) {
    0 @ exit
}
