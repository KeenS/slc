//! A handler clause naming no operation of any effect is refused, rather
//! than ignored.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_unknown_clause_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn a_clause_naming_no_operation_is_refused() {
    let (ok, _, stderr) = run(
        "bare",
        "effect Reader { fn config() -> i64; }
        fn f() -> i64 / {Reader} { config() }
        command main | (exit: -i32) / {IO} {
            let v = handle f() { nope(): resume => <1 | resume, config(): resume => <2 | resume };
            <v | println;
            <0 | exit>
        }",
    );
    assert!(!ok);
    assert!(stderr.contains("`nope` is not an operation of any effect"), "{stderr}");

    let (ok, _, stderr) = run(
        "path",
        "fn canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {..E} {
            handle <(,) | program { fs::nope(path): resume => <::0(\"x\") | resume }
        }
        command main | (exit: -i32) / {IO} { <0 | exit> }",
    );
    assert!(!ok);
    assert!(stderr.contains("`fs::nope` is not an operation of any effect"), "{stderr}");
}

#[test]
fn a_clause_for_a_declared_operation_keeps_its_meaning() {
    let (ok, stdout, stderr) = run(
        "declared",
        "effect Reader { fn config() -> i64; }
        fn f() -> i64 / {Reader} { config() }
        command main | (exit: -i32) / {IO} {
            let v = handle f() { config(): resume => <2 | resume };
            <v | println;
            <0 | exit>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "2\n");
}
