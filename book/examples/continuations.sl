// A continuation receives a value. mu names one. -T is its type.

func emit(text: String, out: -String) -> (;) {
    <text | out>
}

func sum_into(out: i64) <- (i64, i64) {
    mu (i64, i64) {
        (a, b) => <(a, b) | add | out>,
    }
}

proc main | (exit: i32) / {IO} {
    <mu i64 { out <= <(2, 3) | add | out> } | println;
    <mu String { out <= <("Hello, ", "SLC") | add | out> } | println;
    <mu String { out <= <("SLC", out) | emit> } | println;
    <(6, 7) | sum_into | println;
    <"shown" | fn(message: String) -> (;) {
        <message | println;
        <0 | exit>
    }>
}
