// Tensor pairs construct with (a, b). Using the pair requires
// projections (fst/snd), which are core co-terms.
// This example demonstrates the pair literal lowering.

command main | (exit: i32) {
    let x = 10;
    (x, 20) | add | println;
    0 | exit⟩
}
