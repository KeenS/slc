//! A cut whose sides nothing has given a type is refused, and so is a
//! `proc` body that is still an unsolved variable.

use std::process::Command;

fn run(name: &str, source: &str) -> (String, bool) {
    let path = std::env::temp_dir().join(format!("slc_unresolved_cut_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (String::from_utf8(out.stderr).unwrap(), out.status.success())
}

#[test]
fn a_proc_body_of_unsolved_type_is_refused() {
    let (stderr, ok) = run(
        "proc_var",
        "proc bad<+T>(x: T) | (k: i32) {\n    x\n}\n\
         proc main | (exit: i32) / {IO} { <0 | exit> }\n",
    );
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("a `command` body must reach a continuation"), "{stderr}");
}

#[test]
fn a_mu_over_an_unsigned_variable_is_refused() {
    let (stderr, ok) = run(
        "mu_var",
        "proc main | (exit: i32) / {IO} {\n    \
         let scrutinee = mu { k <= <0 | exit> };\n    \
         let consumer = mu { x => <0 | exit> };\n    \
         <scrutinee | consumer>;\n    \
         <0 | exit>\n}\n",
    );
    assert!(!ok, "{stderr}");
    assert!(
        stderr.contains("not known to be one") || stderr.contains("must be a consumer"),
        "{stderr}"
    );
}
