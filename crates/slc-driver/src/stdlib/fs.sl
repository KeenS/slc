// `fs`: files. The module carries the context: `fs::read`, not `read`.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: an operation that can fail takes one continuation per outcome and
// activates exactly one. A file handle is a value of its own type, `File` —
// an integer cannot close a file — produced only by `open`.
//
// Touching a file is the `Fs` effect, so a declaration that does says so in
// its row, and something around it answers the effect: `do expr fs::real`,
// which reaches the disk through the runtime's primitives (`__read_file`
// and its siblings), or a handler of the program's own, which need not
// touch the disk at all.

cite list::List;
cite list::List::*;

// What touching a file performs. An operation answers with its outcome,
// one alternative per continuation of the command that performs it: a
// handler's clause runs below the handler, so the commands, not the
// clauses, activate the continuations, and whatever runs next stays
// under the handler.
pub hook Fs {
    func read_file(path: String) -> (String | String);
    func write_file(path: String, contents: String) -> (unit | String);
    func open_file(path: String) -> (File | String);
    func read_line_of(file: File) -> (String | unit);
    func close_file(file: File) -> (,);
    func file_exists(path: String) -> Bool;
    func read_directory(path: String) -> (List<String> | String);
    func remove_path(path: String) -> (unit | String);
    func rename_path(from: String, to: String) -> (unit | String);
}

// The whole file, or why not.
pub proc read<E>(path: String) | (ok: (-String / {..E}) & failed: (-String / {..E})) / {Fs, ..E} {
    <path | read_file | (ok & failed)>
}

// Replace a file's contents, or say why not.
pub proc write<E>(path: String, contents: String) | (
    ok: (-unit / {..E})
    & failed: (-String / {..E})
) / {Fs, ..E} {
    <(path, contents) | write_file | (ok & failed)>
}

// A handle to read line by line, or why not.
pub proc open<E>(path: String) | (opened: (-File / {..E}) & failed: (-String / {..E})) / {Fs, ..E} {
    <path | open_file | (opened & failed)>
}

// The next line, or the end of the file.
pub proc read_line<E>(file: File) | (line: (-String / {..E}) & end: (-unit / {..E})) / {Fs, ..E} {
    <file | read_line_of | (line & end)>
}

// Spend the handle: a later read through it fails. Composing a close
// onto an exit closes paths routed through that wrapper, not earlier
// captured exits; see `examples/programs/file_io.sl`.
pub func close(file: File) -> (,) / {Fs} {
    <file | close_file
}

pub func exists(path: String) -> Bool / {Fs} {
    <path | file_exists
}

// The names in a directory, in order, or why not. `.` and `..` are not
// among them. The order is the order of the names as strings.
pub proc read_dir<E>(path: String) | (
    ok: (-List<String> / {..E})
    & failed: (-String / {..E})
) / {Fs, ..E} {
    <path | read_directory | (ok & failed)>
}

// Remove a file, or an empty directory, or say why not.
pub proc remove<E>(path: String) | (ok: (-unit / {..E}) & failed: (-String / {..E})) / {Fs, ..E} {
    <path | remove_path | (ok & failed)>
}

pub proc rename<E>(from: String, to: String) | (
    ok: (-unit / {..E})
    & failed: (-String / {..E})
) / {Fs, ..E} {
    <(from, to) | rename_path | (ok & failed)>
}

func names_from(i: i64, n: i64) -> List<String> / {IO} {
    of (<(i, n) | ge) {
        True => Nil,
        False => Cons(<i | __dir_entry, <(<(i, 1) | add, n) | names_from),
    }
}

// The file system itself. `do expr fs::real` answers every operation of
// `Fs` from the disk. The clauses perform `IO`; whatever else `expr`
// performs passes outward. A value and a command use the same hand.
pub hand real / {IO} {
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
    read_directory(path): resume => <path
        | __read_dir
        | (fn(n: i64) { ::0(<(0, n) | names_from) } & fn(why: String) { ::1(why) })
        | resume,
    remove_path(path): resume => <path
        | __remove_path
        | (fn(done: unit) { ::0(done) } & fn(why: String) { ::1(why) })
        | resume,
    rename_path(from, to): resume => <(from, to)
        | __rename_path
        | (fn(done: unit) { ::0(done) } & fn(why: String) { ::1(why) })
        | resume,
}
