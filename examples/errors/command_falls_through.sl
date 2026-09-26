// This program is intentionally ill-typed: the command `bad` reaches no
// continuation. Its body is just the value `x`, so control falls off the
// end instead of ending in a cut.
//
// A `proc` body is `(;)` — every terminating path leaves through a
// continuation. (The core is classical, so *which* continuation, and how
// many, is up to the program; only reaching one is required.) Run it to see
// the diagnostic:
//
//   slc run examples/errors/command_falls_through.sl

proc bad(x: i32) | (k: i32) {
    x
}

proc main | (exit: i32) {
    <0 | exit>
}
