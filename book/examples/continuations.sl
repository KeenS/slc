// `resumed` is the rest of the product. A zero cuts to `stopped`, and that rest never runs.

cite list::List;
cite list::List::*;

func product(resumed: String & stopped: String) <- List<i64> / {IO} {
    fn(xs: List<i64>) -> (;) {
        of xs {
            Nil => <"1" | resumed>,
            Cons(n, rest) => {
                <("visit ", <n | int_to_str) | add | println;
                of n {
                    0 => <"0" | stopped>,
                    _ => {
                        let prepend = fn(tail: String) -> (;) {
                            let factor = <n | int_to_str;
                            let suffix = <("*", tail) | add;
                            let joined = <(factor, suffix) | add;
                            <joined | resumed>
                        };
                        <rest | (<(prepend, stopped) | product)>
                    },
                }
            },
        }
    }
}

func report(xs: List<i64>) -> (,) / {IO} {
    <mu String {
        out <= {
            let resumed = fn(text: String) -> (;) { <("resumed ", text) | add | out> };
            let stopped = fn(text: String) -> (;) { <("stopped ", text) | add | out> };
            <xs | (<(resumed, stopped) | product)>
        },
    } | println
}

proc main | (exit: i32) / {IO} {
    <Nil | report;
    <Cons(2, Cons(3, Nil)) | report;
    <Cons(2, Cons(0, Cons(9, Nil))) | report;
    <0 | exit>
}
