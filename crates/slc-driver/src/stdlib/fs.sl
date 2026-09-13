// `fs`: files.
//
// Reading a file has more than one outcome, so nothing here returns a
// result: an operation that can fail takes one continuation per outcome and
// activates exactly one. A file handle is a value of its own type, `File` —
// an integer cannot close a file — produced only by `open_file`.
//
// These are the whole surface. The runtime's primitives underneath are
// `__read_file` and its siblings; they are what the language cannot express,
// and this module is what a program is meant to call.

mod fs {
    // The whole file, or why not.
    pub command read_file(path: String) | (ok: String & failed: String) / {IO} {
        path | __read_file | (ok & failed)⟩
    }

    // Replace a file's contents, or say why not.
    pub command write_file(path: String, contents: String) | (ok: unit & failed: String) / {IO} {
        (path, contents) | __write_file | (ok & failed)⟩
    }

    // A handle to read line by line, or why not.
    pub command open_file(path: String) | (opened: File & failed: String) / {IO} {
        path | __open_file | (opened & failed)⟩
    }

    // The next line, or the end of the file.
    pub command read_line(file: File) | (line: String & end: unit) / {IO} {
        file | __read_line | (line & end)⟩
    }

    // Spend the handle: a later read through it fails. Closing on every
    // terminating path is not checked — see `PLAN.md` — so compose the close
    // onto the only door out, as `examples/file_io.sl` does.
    pub fn close_file(file: File) -> Unit / {IO} {
        file | __close_file
    }

    pub fn file_exists(path: String) -> bool / {IO} {
        path | __file_exists
    }
}
