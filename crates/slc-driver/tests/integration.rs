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

#[test]
fn short_circuit_and_does_not_evaluate_rhs() {
    let dir = std::env::temp_dir().join("slc_test_short_circuit.sl");
    std::fs::write(&dir, "fn main() -> i32 { if false && (1 / 0 == 1) { 1 } else { 2 } }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("2"));
    assert!(!stderr.contains("division by zero"));
}

#[test]
fn short_circuit_or_does_not_evaluate_rhs() {
    let dir = std::env::temp_dir().join("slc_test_short_circuit_or.sl");
    std::fs::write(&dir, "fn main() -> i32 { if true || (1 / 0 == 1) { 3 } else { 4 } }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("3"));
    assert!(!stderr.contains("division by zero"));
}

#[test]
fn subtraction_is_left_associative() {
    let dir = std::env::temp_dir().join("slc_test_assoc.sl");
    std::fs::write(&dir, "fn main() -> i32 { 10 - 3 - 2 }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn boolean_precedence_below_comparisons() {
    let dir = std::env::temp_dir().join("slc_test_bool_prec.sl");
    std::fs::write(&dir, "fn main() -> i32 { if 1 == 1 || 2 == 3 { 5 } else { 6 } }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn list_indexing_works() {
    let dir = std::env::temp_dir().join("slc_test_list_index.sl");
    std::fs::write(
        &dir,
        "fn main() -> i32 { let xs = list_push(list_push(list_new(), 10), 20); xs[1] }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("20"));
}

#[test]
fn out_of_range_index_rejected() {
    let dir = std::env::temp_dir().join("slc_test_oob.sl");
    std::fs::write(&dir, "fn main() -> i32 { let xs = list_push(list_new(), 10); xs[5] }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("out of range"));
}

#[test]
fn slice_bounds_checked() {
    let dir = std::env::temp_dir().join("slc_test_slice_bounds.sl");
    std::fs::write(&dir, r#"fn main() -> i32 { "abc"[1..9] }"#).unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("out of bounds"));
}

#[test]
fn named_error_propagation_success_path() {
    let dir = std::env::temp_dir().join("slc_test_named_error_ok.sl");
    std::fs::write(
        &dir,
        r#"command parse(input: +String, to ok: -String, to err: -String) {
            if input == "ok" { ok("parsed") } else { err("failed") }
        }
        fn main() -> i32 {
            mu(ret: -i32) {
                let ok = fn(value: +String) -> i32 { ret(0) };
                let err = fn(message: +String) -> i32 { ret(1) };
                parse("ok", ok, err)?err
            }
        }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "expected success, stderr: {stderr}");
}

#[test]
fn named_error_propagation_error_path() {
    let dir = std::env::temp_dir().join("slc_test_named_error_err.sl");
    std::fs::write(
        &dir,
        r#"command parse(input: +String, to ok: -String, to err: -String) {
            if input == "ok" { ok("parsed") } else { err("failed") }
        }
        fn main() -> i32 {
            mu(ret: -i32) {
                let ok = fn(value: +String) -> i32 { ret(0) };
                let err = fn(message: +String) -> i32 { ret(1) };
                parse("bad", ok, err)?err
            }
        }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "expected error continuation to run, stderr: {stderr}");
}

#[test]
fn error_propagation_inside_nested_fn_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_nested_error_prop.sl");
    std::fs::write(
        &dir,
        r#"command parse(input: +String, to ok: -String, to err: -String) {
            if input == "ok" { ok("parsed") } else { err("failed") }
        }
        fn main() -> i32 {
            mu(ret: -i32) {
                let inner = fn(unit: +i32) -> i32 { parse("bad", ret, ret)? };
                inner()
            }
        }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("`?` requires a current error continuation"));
}

#[test]
fn json_selected_error_continuation_reports_parse_error() {
    let dir = std::env::temp_dir().join("slc_test_json_selected_error.sl");
    std::fs::write(
        &dir,
        r#"fn parse_json(input: +String, ok: +String, err: +String) -> i64 {
            let start = skip_ws(input, 0);
            if start < str_len(input) {
                match input[start] {
                    '0'..='9' => ok(input[start..start + 1]),
                    _ => err("expected JSON value")
                }
            } else {
                err("empty input")
            }
        }
        fn main() -> i32 {
            mu(ret: -i32) {
                let ok = fn(value: +String) -> i32 { println("parsed: " + value); ret(0) };
                let err = fn(message: +String) -> i32 { println("error: " + message); ret(1) };
                parse_json("x", ok, err)?err
            }
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("error: expected JSON value"));
}
