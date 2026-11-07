//! Programs from the review, run on both the native compiler and the interpreter.

use std::process::Command;

fn run_both(name: &str, source: &str) -> ((String, String, bool), (String, String, bool)) {
    let path = std::env::temp_dir().join(format!("slc_reported_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let once = |interpret: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_slc"));
        cmd.arg("run");
        if interpret {
            cmd.arg("--interpret");
        }
        cmd.arg(&path);
        let out = cmd.output().expect("failed to run slc");
        (
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
            out.status.success(),
        )
    };
    let elf = once(false);
    let interpreted = once(true);
    let _ = std::fs::remove_file(&path);
    (elf, interpreted)
}

fn agree(name: &str, source: &str, stdout: &str) {
    let (elf, interpreted) = run_both(name, source);
    assert!(elf.2, "{name} native stderr: {}", elf.1);
    assert_eq!(elf.0, stdout, "{name} native stdout");
    assert!(interpreted.2, "{name} interpret stderr: {}", interpreted.1);
    assert_eq!(interpreted.0, stdout, "{name} interpret stdout");
}

#[test]
fn a_backslashed_n_stays_a_backslash_and_an_n() {
    // The lowered literal is Rust's `Debug` form. `\\` is one backslash,
    // so the `n` after it is not a newline.
    agree(
        "backslash_n",
        r#"proc main | (exit: -i32) / {IO} { <"a\\nb" | println; <0 | exit> }"#,
        "a\\nb\n",
    );
}

#[test]
fn an_inverted_range_matches_nothing() {
    agree(
        "inverted_range",
        "proc main | (exit: -i32) / {IO} {
            let n = of 3 { 5..=1 => 1, 3 => 2, _ => 0 };
            <n | println;
            <0 | exit>
        }",
        "2\n",
    );
}

#[test]
fn adding_a_call_agrees_with_the_interpreter() {
    agree(
        "add_call",
        "func g() -> i64 { 40 }
         func f() -> i64 { <(g(), 2) | __add }
         proc main | (exit: -i32) / {IO} {
             <f() | println;
             let n = <(g(), 2) | __add;
             <n | println;
             <0 | exit>
         }",
        "42\n42\n",
    );
}

#[test]
fn a_generated_binder_does_not_capture_the_parameter() {
    agree(
        "capture",
        r#"func len(__call: String) -> i64 { <__call | str_len }
           func pick(__match: i64) -> i64 { of __match { 1 => 7, _ => 0 } }
           func pass(__cut: i64) -> i64 { mu i64 { k <= <__cut | k> } }
           func first(__prj: (i64, i64)) -> i64 { __prj.0 }
           func keep(__tail: i64) -> i64 { let x = 1; x }
           proc main | (exit: -i32) / {IO} {
               <"hi" | len | println;
               <1 | pick | println;
               <7 | pass | println;
               <(1, 2) | first | println;
               <0 | keep | println;
               <0 | exit>
           }"#,
        "2\n7\n7\n1\n1\n",
    );
}
