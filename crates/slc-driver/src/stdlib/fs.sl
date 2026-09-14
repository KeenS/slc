// `fs`: files. The module carries the context: `fs::read`, not `read`.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: an operation that can fail takes one continuation per outcome and
// activates exactly one. A file handle is a value of its own type, `File` —
// an integer cannot close a file — produced only by `open`.
//
// Touching a file is the `Fs` effect, so a declaration that does says so in
// its row, and something around it answers the effect: `fs::real`, which
// reaches the disk through the runtime's primitives (`__read_file` and its
// siblings), or a handler of the program's own, which need not touch the disk
// at all.

mod fs {
    // What touching a file performs. An operation answers with its outcome,
    // one alternative per continuation of the command that performs it: a
    // handler's clause runs below the handler, so the commands, not the
    // clauses, activate the continuations, and whatever runs next stays
    // under the handler.
    pub effect Fs {
        fn read_file(path: String) -> (String | String);
        fn write_file(path: String, contents: String) -> (unit | String);
        fn open_file(path: String) -> (File | String);
        fn read_line_of(file: File) -> (String | unit);
        fn close_file(file: File) -> (,);
        fn file_exists(path: String) -> Bool;
    }

    // The whole file, or why not.
    pub command read(path: String) | (ok: String & failed: String) / {Fs} {
        <path | read_file | (ok & failed)>
    }

    // Replace a file's contents, or say why not.
    pub command write(path: String, contents: String) | (ok: unit & failed: String) / {Fs} {
        <(path, contents) | write_file | (ok & failed)>
    }

    // A handle to read line by line, or why not.
    pub command open(path: String) | (opened: File & failed: String) / {Fs} {
        <path | open_file | (opened & failed)>
    }

    // The next line, or the end of the file.
    pub command read_line(file: File) | (line: String & end: unit) / {Fs} {
        <file | read_line_of | (line & end)>
    }

    // Spend the handle: a later read through it fails. Every path closes
    // the file by construction when the close is composed onto the only door
    // out, as `examples/file_io.sl` does.
    pub fn close(file: File) -> (,) / {Fs} {
        <file | close_file
    }

    pub fn exists(path: String) -> Bool / {Fs} {
        <path | file_exists
    }

    // The file system itself. It runs `program`, answers every operation of
    // `Fs` it performs with the runtime's primitive, and passes on whatever
    // else it performs.
    pub fn real<+A, E>(program: ((,) -> A / {Fs, ..E})) -> A / {IO, ..E} {
        handle <(,) | program {
            // The primitive offers its outcome to one of two consumers; each
            // resumes with that outcome, and hands what the resumed program
            // produces to `out`, the clause's own continuation.
            read_file(path): resume => mu { out <= <path | __read_file | (
                select String { text => <(<::0(text) | resume) | out> }
                & select String { why => <(<::1(why) | resume) | out> }
            )> },
            write_file(path, contents): resume => mu { out <= <(path, contents) | __write_file | (
                select unit { done => <(<::0(done) | resume) | out> }
                & select String { why => <(<::1(why) | resume) | out> }
            )> },
            open_file(path): resume => mu { out <= <path | __open_file | (
                select File { file => <(<::0(file) | resume) | out> }
                & select String { why => <(<::1(why) | resume) | out> }
            )> },
            read_line_of(file): resume => mu { out <= <file | __read_line | (
                select String { line => <(<::0(line) | resume) | out> }
                & select unit { end => <(<::1(end) | resume) | out> }
            )> },
            close_file(file): resume => <(<file | __close_file) | resume,
            file_exists(path): resume => <(<path | __file_exists) | resume,
        }
    }
}
