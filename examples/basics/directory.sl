// The names in a directory, then a rename, then removal. The directory is
// created by `/bin/mkdir`. Names come back in string order, without `.` or
// `..`. Leftovers from an earlier run are removed first.

cite list::List::*;

func join(dir: String, name: String) -> String {
    <(<(dir, "/") | add, name) | add
}

// A missing path is the same as a removed one: both outcomes continue.
func drop_path(done: (,)) <- String / {fs::Fs} {
    fs::remove | (done & fn(_: String) { <(,) | done> })>
}

proc main | (exit: i32) / {IO} {
    do {
        do {
            let dir = "/tmp/slc-deferred-dir";
            <(<(dir, "a.txt") | join) | drop_path;
            <(<(dir, "b.txt") | join) | drop_path;
            <(<(dir, "c.txt") | join) | drop_path;
            <dir | drop_path;

            let made = mu i64 {
                status <= <("/bin/mkdir", Cons("-p", Cons(dir, Nil)))
                    | process::spawn
                    | (status & mu String {
                        _ => {
                            <"cannot spawn" | println;
                            <1 | exit>
                        },
                    })>,
            };
            <made | println;

            let wrote_b = mu unit {
                done <= <(<(dir, "b.txt") | join, "b") | fs::write | (done & mu String {
                    why => {
                        <why | println;
                        <1 | exit>
                    },
                })>,
            };
            let wrote_a = mu unit {
                done <= <(<(dir, "a.txt") | join, "a") | fs::write | (done & mu String {
                    why => {
                        <why | println;
                        <1 | exit>
                    },
                })>,
            };
            of wrote_b { _ => (,) };
            of wrote_a { _ => (,) };

            let names = mu list::List<String> {
                ok <= <dir | fs::read_dir | (ok & mu String {
                    why => {
                        <why | println;
                        <1 | exit>
                    },
                })>,
            };
            <names | println;

            let moved = mu unit {
                done <= <(<(dir, "a.txt") | join, <(dir, "c.txt") | join)
                    | fs::rename
                    | (done & mu String {
                        why => {
                            <why | println;
                            <1 | exit>
                        },
                    })>,
            };
            of moved { _ => (,) };

            let renamed = mu list::List<String> {
                ok <= <dir | fs::read_dir | (ok & mu String {
                    why => {
                        <why | println;
                        <1 | exit>
                    },
                })>,
            };
            <renamed | println;

            <(<(dir, "c.txt") | join) | drop_path;
            <(<(dir, "b.txt") | join) | drop_path;
            let cleared = mu unit {
                done <= <dir | fs::remove | (done & mu String {
                    why => {
                        <why | println;
                        <1 | exit>
                    },
                })>,
            };
            of cleared { _ => (,) };
            <"clear" | println;
            <0 | exit>
        } process::real
    } fs::real
}
