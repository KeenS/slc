// ── Taps ─────────────────────────────────────────────────────────────────
//
// A helper that takes both values and continuations is a `command`: that is
// what the declaration square calls the shape, and the header says it — the
// value group before the `|`, the menu of exits after. The older spelling —
// a positive `fn` returning `-T`, built as a λ whose body is a command — was
// the same type by `A → ⊥` *is* `-A`, but it said the shape only in the
// return position, and the caller had to build the consumer before cutting
// into it. A command is written and read the way every other call is:
//
//     ("answer", 42) | trace::tap | out⟩

mod trace {
    // A tap: log a label and the value passing through, then forward it.
    pub command tap<T>(label: String, x: T) | (k: T) / {IO} {
        ⟨label | println;
        ⟨x | println;
        ⟨x | k⟩
    }
}
