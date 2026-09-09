// This program is intentionally ill-typed: it writes the two polarity
// combinations that are rejected. `polarity.sl` writes the four that are not.
//
// Run it to see the diagnostics:
//
//   slc run examples/polarity_error.sl

// A `mu` splits its parameters by polarity, so a consumer cannot be a value
// parameter: `x` belongs in the second group.
command bad_value(x: -i32) | (k: -i32) {
    0 @ k
}

// ...and a value cannot be a continuation parameter: control cannot leave
// through something that is not a consumer.
command bad_continuation | (j: +i32) {
    0
}

command main | (exit: -i32) {
    0 @ exit
}
