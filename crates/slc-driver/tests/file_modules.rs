//! `sect name;`: a module whose body is a file of its own.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A directory of sources, written fresh, and the path of its `main.sl`.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slc_file_modules_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (file, source) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    dir.join("main.sl")
}

fn slc(subcommand: &str, main: &Path) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg(subcommand)
        .arg(main)
        .output()
        .expect("failed to run slc");
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

const MAIN: &str = "sect geometry;

cite geometry::area;

proc main | (exit: i32) / {IO} {
    <geometry::Shape::Rect(6, 7) | area | println;
    <0 | exit>
}
";

const GEOMETRY: &str = "pub enum Shape { Circle(i64), Rect(i64, i64) }

cite Shape::*;

func squared(n: i64) -> i64 { <(n, n) | mul }

pub func area(s: Shape) -> i64 {
    of s {
        Circle(r) => <(3, <r | squared) | mul,
        Rect(w, h) => <(w, h) | mul,
    }
}
";

#[test]
fn a_module_is_loaded_from_the_file_beside_the_program() {
    let main = project("beside", &[("main.sl", MAIN), ("geometry.sl", GEOMETRY)]);
    let (ok, stdout, stderr) = slc("run", &main);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "42\n");
    assert!(slc("check", &main).0);
}

#[test]
fn a_module_file_keeps_its_private_names() {
    let main = project(
        "private",
        &[
            (
                "main.sl",
                "sect geometry;\nproc main | (exit: i32) / {IO} {\n    <4 | geometry::squared | println;\n    <0 | exit>\n}\n",
            ),
            ("geometry.sl", GEOMETRY),
        ],
    );
    let (ok, _, stderr) = slc("run", &main);
    assert!(!ok);
    assert!(stderr.contains("squared") && stderr.contains("private"), "{stderr}");
}

#[test]
fn a_module_file_declares_its_own_modules_a_directory_down() {
    // `mod shapes;` in geometry.sl is geometry/shapes.sl, and one inside an
    // inline `mod outer { … }` of the program is outer/inner.sl.
    let main = project(
        "nested",
        &[
            (
                "main.sl",
                "sect geometry;\nsect outer { pub sect inner; }\nproc main | (exit: i32) / {IO} {\n    <geometry::shapes::unit() | println;\n    <outer::inner::deep() | println;\n    <0 | exit>\n}\n",
            ),
            ("geometry.sl", "pub sect shapes;\n"),
            ("geometry/shapes.sl", "pub func unit() -> i64 { 1 }\n"),
            ("outer/inner.sl", "pub func deep() -> i64 { 2 }\n"),
        ],
    );
    let (ok, stdout, stderr) = slc("run", &main);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "1\n2\n");
}

#[test]
fn a_module_file_reaches_the_library_and_the_program_s_menus() {
    // The library a module file names is loaded, and a `menu` the program
    // declares is known while the module file is parsed: `mu Pair { … }`
    // there is a menu value, not a binder.
    let main = project(
        "library",
        &[
            (
                "main.sl",
                "sect tools;\nmenu Offer { price: i64 }\nproc main | (exit: i32) / {IO} {\n    <tools::total() | println;\n    <tools::offer().price | println;\n    <0 | exit>\n}\n",
            ),
            (
                "tools.sl",
                "cite list::List::*;\npub func total() -> i64 { <Cons(1, Cons(2, Nil)) | list::length }\npub func offer() -> Offer { mu Offer { price <= <9 | price> } }\n",
            ),
        ],
    );
    let (ok, stdout, stderr) = slc("run", &main);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "2\n9\n");
}

#[test]
fn a_diagnostic_in_a_module_file_names_the_file_and_its_own_line() {
    let main = project(
        "diagnostic",
        &[
            ("main.sl", "sect broken;\nproc main | (exit: i32) { <0 | exit> }\n"),
            ("broken.sl", "// a comment line\n\npub func f() -> i64 { \"text\" }\n"),
        ],
    );
    let (ok, _, stderr) = slc("check", &main);
    assert!(!ok);
    assert!(stderr.contains("type:"), "{stderr}");
    assert!(stderr.contains("broken.sl:3:"), "{stderr}");
}

#[test]
fn a_syntax_error_in_a_module_file_stays_in_that_file() {
    for (name, source) in [
        ("string", "pub func f() -> String { \"left open }\n"),
        ("brace", "pub func f() -> i64 { 1\n"),
        ("token", "pub func f() -> i64 { 1 + 2 }\n"),
    ] {
        let main = project(
            name,
            &[
                (
                    "main.sl",
                    "sect broken;\nproc main | (exit: i32) / {IO} {\n    <\"fine\" | println;\n    <0 | exit>\n}\n",
                ),
                ("broken.sl", source),
            ],
        );
        let (ok, _, stderr) = slc("run", &main);
        assert!(!ok, "{name} ran");
        assert!(!stderr.contains("prelude.sl") && !stderr.contains("main.sl"), "{name}: {stderr}");
    }
    let main = project(
        "located",
        &[
            ("main.sl", "sect broken;\nproc main | (exit: i32) { <0 | exit> }\n"),
            ("broken.sl", "pub func f() -> i64 { 1 + 2 }\n"),
        ],
    );
    let (_, _, stderr) = slc("run", &main);
    assert!(stderr.contains("broken.sl:1:25 `+`"), "{stderr}");
}

#[test]
fn a_module_with_no_file_is_reported_where_it_is_declared() {
    let main = project(
        "missing",
        &[("main.sl", "\nsect nowhere;\nproc main | (exit: i32) { <0 | exit> }\n")],
    );
    let (ok, _, stderr) = slc("run", &main);
    assert!(!ok);
    assert!(stderr.contains("nowhere.sl"), "{stderr}");
    assert!(stderr.contains("(at 2:1 `sect nowhere;`)"), "{stderr}");
}

#[test]
fn a_file_is_not_loaded_twice() {
    let main = project(
        "twice",
        &[
            ("main.sl", "sect a;\nsect a;\nproc main | (exit: i32) { <0 | exit> }\n"),
            ("a.sl", "pub func f() -> i64 { 1 }\n"),
        ],
    );
    let (ok, _, stderr) = slc("run", &main);
    assert!(!ok);
    assert!(stderr.contains("already"), "{stderr}");
}

#[test]
fn a_variant_import_is_scoped_to_the_file_that_wrote_it() {
    // Two enums declare `Rect`, so a bare `Rect` needs an import to pin it,
    // and each file's import pins it for that file alone: geometry.sl's
    // `use Shape::*;` and the program's `use Frame::*;` never meet.
    let frame = "sect geometry;\nenum Frame { Rect(i64, i64) }\n";
    let main = "func width(f: Frame) -> i64 { of f { Rect(w, h) => w } }
proc main | (exit: i32) / {IO} {
    <Rect(6, 7) | width | println;
    <geometry::Shape::Rect(6, 7) | geometry::area | println;
    <0 | exit>
}
";
    let pinned = project(
        "pinned",
        &[("main.sl", &format!("{frame}cite Frame::*;\n{main}")), ("geometry.sl", GEOMETRY)],
    );
    let (ok, stdout, stderr) = slc("run", &pinned);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "6\n42\n");

    // Without its own import the program's `Rect` is ambiguous: the module
    // file's import does not reach it.
    let unpinned =
        project("unpinned", &[("main.sl", &format!("{frame}{main}")), ("geometry.sl", GEOMETRY)]);
    let (ok, _, stderr) = slc("run", &unpinned);
    assert!(!ok, "the module file's import pinned the program's `Rect`");
    assert!(stderr.contains("Rect"), "{stderr}");
}
