use std::process::Command;

fn run_sl(path: &str) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .args(["run", path])
        .output()
        .expect("failed to run slc");
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.success(),
    )
}

#[test]
fn run_int_main() {
    let dir = std::env::temp_dir().join("slc_test_int.sl");
    std::fs::write(&dir, "fn main() -> i32 { 42 }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn polarity_error() {
    let dir = std::env::temp_dir().join("slc_test_pol.sl");
    std::fs::write(&dir, "fn bad(x: -i32) -> i32 { x }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("polarity"));
}

#[test]
fn linearity_error() {
    let dir = std::env::temp_dir().join("slc_test_lin.sl");
    std::fs::write(&dir, "command bad(x: +i32, to k: -i32) { x }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("linearity"));
}

#[test]
fn no_input_file() {
    let out = Command::new(env!("CARGO_BIN_EXE_slc")).output().expect("failed to run slc");
    assert!(!out.status.success());
}

#[test]
fn builtin_add() {
    let dir = std::env::temp_dir().join("slc_test_add.sl");
    std::fs::write(&dir, "fn main() -> i32 { add(1, 2) }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("3"));
}

#[test]
fn builtin_println() {
    let dir = std::env::temp_dir().join("slc_test_println.sl");
    std::fs::write(&dir, "fn main() -> i32 { println(42) }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn lambda_application() {
    let dir = std::env::temp_dir().join("slc_test_lambda.sl");
    std::fs::write(&dir, "fn main() -> i32 { fn(x: +i32) -> i32 { x }(5) }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn string_operations() {
    let dir = std::env::temp_dir().join("slc_test_str.sl");
    std::fs::write(&dir, "fn main() -> i32 { str_concat(int_to_str(1), int_to_str(2)) }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("12"));
}

#[test]
fn comparison() {
    let dir = std::env::temp_dir().join("slc_test_cmp.sl");
    std::fs::write(&dir, "fn main() -> i32 { eq(1, 1) }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("true"));
}

#[test]
fn division_by_zero_rejected() {
    let dir = std::env::temp_dir().join("slc_test_div.sl");
    std::fs::write(&dir, "fn main() -> i32 { div(1, 0) }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("division by zero"));
}

#[test]
fn parse_int_multi_continuation_success() {
    let dir = std::env::temp_dir().join("slc_test_parse_ok.sl");
    std::fs::write(
        &dir,
        r#"fn main() -> i32 {
    mu(ret: -i32) {
        let ok = fn(n: +i64) -> i32 { println(n); ret(1) };
        let empty = fn(s: +String) -> i32 { println(s); ret(2) };
        let overflow = fn(s: +String) -> i32 { println(s); ret(3) };
        __parse_int("42", ok, empty, overflow)
    }
}"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stdout: {stdout}");
    assert!(stdout.contains("42"));
}

#[test]
fn parse_int_multi_continuation_empty() {
    let dir = std::env::temp_dir().join("slc_test_parse_empty.sl");
    std::fs::write(
        &dir,
        r#"fn main() -> i32 {
    mu(ret: -i32) {
        let ok = fn(n: +i64) -> i32 { println(n); ret(1) };
        let empty = fn(s: +String) -> i32 { println(s); ret(2) };
        let overflow = fn(s: +String) -> i32 { println(s); ret(3) };
        __parse_int("", ok, empty, overflow)
    }
}"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stdout: {stdout}");
    assert!(stdout.contains("2"));
}

#[test]
fn parse_int_multi_continuation_overflow() {
    let dir = std::env::temp_dir().join("slc_test_parse_overflow.sl");
    std::fs::write(
        &dir,
        r#"fn main() -> i32 {
    mu(ret: -i32) {
        let ok = fn(n: +i64) -> i32 { println(n); ret(1) };
        let empty = fn(s: +String) -> i32 { println(s); ret(2) };
        let overflow = fn(s: +String) -> i32 { println(s); ret(3) };
        __parse_int("99999999999999999999999", ok, empty, overflow)
    }
}"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stdout: {stdout}");
    assert!(stdout.contains("3"));
}
