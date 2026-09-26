// `Hash`: a non-negative `u64` for a value.
//
// Equal values hash equal. The same machine word hashes equal across the
// integer widths. A negative `i64` still hashes non-negative, so `rem` by a
// small width lands in `0 .. width`.

func hash_u(n: u64) -> u64 { <n | hash }

func show<+T: Hash>(x: T) -> u64 { <x | hash }

proc main | (exit: i32) / {IO} {
    <0 | hash | println;
    <42 | hash | println;
    <-1 | hash | println;
    <42 | hash_u | println; // the same word as i64 42
    <42 | show | println;
    <'a' | hash | println;
    <'b' | hash | println;
    <False | hash | println; // 0 — False mixes the word 0
    <True | hash | println;
    <"" | hash | println;
    <"ab" | hash | println;
    <"ab" | show | println; // equal string, same hash
    <"ba" | hash | println;
    <0 | exit>
}
