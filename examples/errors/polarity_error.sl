// This program is intentionally ill-typed: it writes the one polarity
// combination that is rejected. `polarity.sl` writes the ones that are not —
// a consumer is a value, so it may arrive as a value parameter; the rule
// that remains is about leaving.
//
// Run it to see the diagnostic:
//
//   slc run examples/errors/polarity_error.sl

// A value cannot be a continuation parameter: control cannot leave through
// something that is not a consumer. (`k` gives the body a real continuation
// to reach, so the error left is the one about `j`.)
command bad_continuation | (j: +i32 & k: -i32) {
    <0 | k>
}

command main | (exit: -i32) {
    <0 | exit>
}
