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
    pub command read<E>(path: String) | (
        ok: (-String / {..E})
        & failed: (-String / {..E})
    ) / {Fs, ..E} {
        <path | read_file | (ok & failed)>
    }

    // Replace a file's contents, or say why not.
    pub command write<E>(path: String, contents: String) | (
        ok: (-unit / {..E})
        & failed: (-String / {..E})
    ) / {Fs, ..E} {
        <(path, contents) | write_file | (ok & failed)>
    }

    // A handle to read line by line, or why not.
    pub command open<E>(path: String) | (
        opened: (-File / {..E})
        & failed: (-String / {..E})
    ) / {Fs, ..E} {
        <path | open_file | (opened & failed)>
    }

    // The next line, or the end of the file.
    pub command read_line<E>(file: File) | (
        line: (-String / {..E})
        & end: (-unit / {..E})
    ) / {Fs, ..E} {
        <file | read_line_of | (line & end)>
    }

    // Spend the handle: a later read through it fails. Composing a close
    // onto an exit closes paths routed through that wrapper, not earlier
    // captured exits; see `examples/programs/file_io.sl`.
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
            read_file(path): resume => <path
                | __read_file
                | (fn(text: String) { ::0(text) } & fn(why: String) { ::1(why) })
                | resume,
            write_file(path, contents): resume => <(path, contents)
                | __write_file
                | (fn(done: unit) { ::0(done) } & fn(why: String) { ::1(why) })
                | resume,
            open_file(path): resume => <path
                | __open_file
                | (fn(file: File) { ::0(file) } & fn(why: String) { ::1(why) })
                | resume,
            read_line_of(file): resume => <file
                | __read_line
                | (fn(line: String) { ::0(line) } & fn(end: unit) { ::1(end) })
                | resume,
            close_file(file): resume => <(<file | __close_file) | resume,
            file_exists(path): resume => <(<path | __file_exists) | resume,
        }
    }

    // The same file system, for a program that leaves through continuations
    // of its own: `fn { …; <0 | exit> }` has type `(;)`, so it is handed to a
    // command as its exit, `<(,) | fs::real_command | (fn { … })>`. The
    // clauses are `real`'s; `real` cannot run through this command, since its
    // own continuation was captured outside the handler.
    pub command real_command<E> | (program: ((;) / {Fs, ..E})) / {IO, ..E} {
        handle program {
            read_file(path): resume => <path
                | __read_file
                | (fn(text: String) { ::0(text) } & fn(why: String) { ::1(why) })
                | resume,
            write_file(path, contents): resume => <(path, contents)
                | __write_file
                | (fn(done: unit) { ::0(done) } & fn(why: String) { ::1(why) })
                | resume,
            open_file(path): resume => <path
                | __open_file
                | (fn(file: File) { ::0(file) } & fn(why: String) { ::1(why) })
                | resume,
            read_line_of(file): resume => <file
                | __read_line
                | (fn(line: String) { ::0(line) } & fn(end: unit) { ::1(end) })
                | resume,
            close_file(file): resume => <(<file | __close_file) | resume,
            file_exists(path): resume => <(<path | __file_exists) | resume,
        }
    }
}
