//! `slc check`: every compiler phase, and no run.

use std::path::PathBuf;
use std::process::{Command, Output};

fn scratch(name: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("slc_check_{}_{name}.sl", std::process::id()));
    std::fs::write(&path, source).unwrap();
    path
}

fn slc_check(files: &[&PathBuf]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("check")
        .args(files)
        .output()
        .expect("failed to run slc")
}

const PRINTS: &str =
    "command main | (exit: i32) / {IO} {\n    <\"ran\" | println;\n    <0 | exit>\n}\n";

#[test]
fn a_well_typed_program_passes_and_is_not_run() {
    let out = slc_check(&[&scratch("ok", PRINTS)]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty(), "checking ran the program");
    assert!(out.stderr.is_empty());
}

#[test]
fn a_program_that_would_not_stop_is_still_checked() {
    let source = "fn spin(n: i32) -> i32 { <n | spin }\n\
                  command main | (exit: i32) / {IO} { <(<0 | spin) | exit> }\n";
    assert!(slc_check(&[&scratch("spin", source)]).status.success());
}

#[test]
fn each_phase_reports_as_it_does_under_run() {
    for (name, source, expected) in [
        ("parse", "fn f() -> i64 { a && b }", "parse error:"),
        ("type", "fn f() -> i64 { \"text\" }", "type:"),
        (
            "exhaustive",
            "enum C { A, B }\nfn f(c: C) -> i64 { match c { A => 1 } }",
            "exhaustiveness:",
        ),
    ] {
        let path = scratch(name, source);
        let out = slc_check(&[&path]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{name} passed");
        assert!(stderr.contains(expected), "{name}: {stderr}");
        assert!(stderr.contains(path.to_str().unwrap()), "{name} does not name its file: {stderr}");
    }
}

#[test]
fn a_file_with_no_main_is_a_library_and_checks() {
    let out = slc_check(&[&scratch("library", "fn double(n: i64) -> i64 { <(n, 2) | mul }\n")]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_main_of_the_wrong_shape_is_refused() {
    let out = slc_check(&[&scratch("main", "fn main() -> i64 { 0 }\n")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("entry point must be"));
}

#[test]
fn every_file_is_checked_and_one_failure_fails_the_command() {
    let good = scratch("good", PRINTS);
    let bad = scratch("bad", "fn f() -> i64 { \"text\" }");
    let out = slc_check(&[&bad, &good]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(bad.to_str().unwrap()), "{stderr}");
    assert!(!stderr.contains(good.to_str().unwrap()), "{stderr}");
}

#[test]
fn check_without_files_prints_the_usage() {
    let out = slc_check(&[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("slc check <file.sl>..."));
}
