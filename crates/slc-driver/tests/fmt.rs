//! `slc fmt` as a command: what it writes, what it refuses, how it exits.

use std::path::PathBuf;
use std::process::{Command, Output};

const UNFORMATTED: &str = "command main|(exit:i32)/{IO}{\n<0|exit>}\n";
const FORMATTED: &str = "command main | (exit: i32) / {IO} {\n    <0 | exit>\n}\n";

fn scratch(name: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("slc-fmt-{}-{name}.sl", std::process::id()));
    std::fs::write(&path, source).expect("a writable temp dir");
    path
}

fn slc_fmt(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("fmt")
        .args(args)
        .output()
        .expect("failed to run slc")
}

#[test]
fn fmt_rewrites_a_file_in_place_and_check_then_passes() {
    let path = scratch("rewrite", UNFORMATTED);
    let file = path.to_str().unwrap();

    let check = slc_fmt(&["--check", file]);
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stderr).contains("is not formatted"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), UNFORMATTED, "`--check` wrote");

    assert!(slc_fmt(&[file]).status.success());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), FORMATTED);
    assert!(slc_fmt(&["--check", file]).status.success());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn stdout_prints_the_layout_and_leaves_the_file_alone() {
    let path = scratch("stdout", UNFORMATTED);
    let out = slc_fmt(&["--stdout", path.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), FORMATTED);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), UNFORMATTED);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_file_that_does_not_parse_is_reported_and_untouched() {
    let broken = "command main | (exit: i32) { if }\n";
    let path = scratch("broken", broken);
    let out = slc_fmt(&[path.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("there is no `if`"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn fmt_without_files_prints_the_usage() {
    let out = slc_fmt(&[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("slc fmt [--check | --stdout]"));
}
