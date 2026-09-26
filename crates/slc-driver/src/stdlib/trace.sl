// ── Taps ─────────────────────────────────────────────────────────────────
//
// A helper that takes both values and continuations is a `proc`: that is
// what the declaration square calls the shape, and the header says it — the
// value group before the `|`, the menu of exits after. The older spelling —
// a positive `func` returning `-T`, built as a λ whose body is a command — was
// the same type by `(A -> (;))` *is* `-A`, but it said the shape only in the
// return position, and the caller had to build the consumer before cutting
// into it. A command is written and read the way every other call is:
//
//     ("answer", 42) | trace::tap | out>

// A tap: log a label and the value passing through, then forward it. What
// passes through is printed, so it has `Display`.
pub proc tap<+T: Display, E>(label: String, x: T) | (k: (-T / {..E})) / {IO, ..E} {
    <label | println;
    <x | println;
    <x | k>
}
