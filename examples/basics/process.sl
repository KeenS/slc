// The process environment, a zero-length wait, and a child process. A missing
// variable is absence. A process that exits is that status, including a
// non-zero one. A program that cannot be started is the other outcome.

cite list::List::*;

proc main | (exit: i32) / {IO} {
    do {
        do {
            do {
                do {
                    let missing = mu String {
                        found <= <"SLC_EXAMPLE_ABSENT" | env::get | (found & mu unit {
                            _ => <"absent" | found>,
                        })>,
                    };
                    <missing | println;

                    let stored = mu unit {
                        done <= <("SLC_EXAMPLE_PRESENT", "kept") | env::set | (done & mu String {
                            why => {
                                <why | println;
                                <1 | exit>
                            },
                        })>,
                    };
                    of stored { _ => (,) };
                    let found = mu String {
                        value <= <"SLC_EXAMPLE_PRESENT" | env::get | (value & mu unit {
                            _ => <"absent" | value>,
                        })>,
                    };
                    <found | println;

                    let before = <(,) | clock::now;
                    <0 | sleep::sleep;
                    let after = <(,) | clock::now;
                    <(<(after, before) | ge) | println;

                    let yes = mu i64 {
                        status <= <("/bin/true", Nil) | process::spawn | (status & mu String {
                            why => {
                                <why | println;
                                <1 | exit>
                            },
                        })>,
                    };
                    <yes | println;

                    let no = mu i64 {
                        status <= <("/bin/false", Nil) | process::spawn | (status & mu String {
                            why => {
                                <why | println;
                                <1 | exit>
                            },
                        })>,
                    };
                    <no | println;

                    let missed = mu String {
                        text <= <("/bin/slc-missing-program", Nil)
                            | process::spawn
                            | (mu i64 { status => <(<status | fmt) | text> } & mu String {
                                _ => <"cannot spawn" | text>,
                            })>,
                    };
                    <missed | println;

                    let echoed = mu i64 {
                        status <= <("/bin/echo", Cons("hello", Nil))
                            | process::spawn
                            | (status & mu String {
                                why => {
                                    <why | println;
                                    <1 | exit>
                                },
                            })>,
                    };
                    <echoed | println;
                    <0 | exit>
                } process::real
            } sleep::real
        } env::real
    } clock::real
}
