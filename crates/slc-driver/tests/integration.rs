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
    std::fs::write(&dir, "command main | (exit: -i32) { println(42); 0 @ exit }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn exit_zero_returns_success() {
    let dir = std::env::temp_dir().join("slc_test_exit_zero.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { 0 @ exit }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "expected `0 @ EXIT` to succeed, stderr: {stderr}");
}

#[test]
fn missing_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_missing_main.sl");
    std::fs::write(&dir, "fn helper() -> i32 { 0 }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("no `main`"), "stderr: {stderr}");
}

#[test]
fn malformed_negative_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_negative_main.sl");
    std::fs::write(&dir, "fn main(k: -i32) <- i32 { 0 @ k }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("entry point must be"), "stderr: {stderr}");
}

#[test]
fn malformed_parameterized_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_parameterized_main.sl");
    std::fs::write(&dir, "fn main(x: +i32) -> i32 { x }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("entry point must be"), "stderr: {stderr}");
}

#[test]
fn malformed_generic_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_generic_main.sl");
    std::fs::write(&dir, "fn main<T>() -> i32 { 0 }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("entry point must be"), "stderr: {stderr}");
}

#[test]
fn the_accepted_entry_point_is_a_command_with_one_exit_continuation() {
    let dir = std::env::temp_dir().join("slc_test_valid_main.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { println(7); 0 @ exit }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("7"), "stdout: {stdout}");

    // A value parameter, or a row that is not one exit status, is rejected.
    for rejected in [
        "command main(x: +i32) | (exit: -i32) { 0 @ exit }",
        "command main | (a: -i32, b: -i32) { if true { 0 @ a } else { 1 @ b } }",
        "command main | (exit: -String) { \"done\" @ exit }",
    ] {
        std::fs::write(&dir, rejected).unwrap();
        let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
        assert!(!ok, "{rejected} was accepted");
        assert!(stderr.contains("entry point must be"), "{rejected}: {stderr}");
    }
}

#[test]
fn exit_nonzero_returns_failure() {
    let dir = std::env::temp_dir().join("slc_test_exit_nonzero.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { 7 @ exit }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected `7 @ EXIT` to fail");
    assert!(!stderr.contains("exit(7)"), "EXIT must not be reported as a diagnostic: {stderr}");
}

#[test]
fn diagnostic_that_merely_looks_like_exit_is_not_treated_as_exit() {
    let dir = std::env::temp_dir().join("slc_test_exit_like_diagnostic.sl");
    std::fs::write(
        &dir,
        r#"fn missing(value: +String) -> i32 { 0 @ exit }
        command main | (exit: -i32) { println(exit_like); 0 @ exit }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("unbound variable: exit_like"), "stderr: {stderr}");
    assert!(!stderr.contains("exit("), "diagnostic must not be parsed as EXIT: {stderr}");
}

#[test]
fn polarity_error() {
    let dir = std::env::temp_dir().join("slc_test_pol.sl");
    std::fs::write(&dir, "command bad(x: -i32) | (k: -i32) { 0 @ k }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("polarity"));
}

#[test]
fn checker_diagnostics_include_source_locations() {
    let cases = [
        (
            "slc_test_location_type.sl",
            "command main | (exit: -i32) { println(1 + true); 0 @ exit }",
            ["type:", "1:39", "`1 + true`"],
        ),
        (
            "slc_test_location_polarity.sl",
            "command bad(x: -i32) | (k: -i32) { 0 @ k }",
            ["polarity:", "1:1", "`command`"],
        ),
        (
            "slc_test_location_linearity.sl",
            "command bad(x: +i32) | (k: -i32) { x }",
            ["linearity:", "1:34", "`{ x }`"],
        ),
    ];

    for (filename, source, expected) in cases {
        let dir = std::env::temp_dir().join(filename);
        std::fs::write(&dir, source).unwrap();
        let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
        assert!(!ok, "{filename}: expected a diagnostic");
        for fragment in expected {
            assert!(stderr.contains(fragment), "{filename}: missing {fragment:?} in {stderr:?}");
        }
    }
}

#[test]
fn earlier_diagnostic_phases_take_precedence_over_later_phases() {
    // This program has both a parse error and, if parsing were repaired, a
    // polarity error. Parsing must report first and must not run checkers.
    let dir = std::env::temp_dir().join("slc_test_phase_parse_precedence.sl");
    std::fs::write(&dir, "command main( | (exit: -i32) { 0 @ exit }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.starts_with("error: parse error:"), "stderr: {stderr}");

    // A valid parse with a type error must stop before polarity checking.
    let dir = std::env::temp_dir().join("slc_test_phase_type_precedence.sl");
    std::fs::write(&dir, "command bad(x: -i32) | (k: -i32) { k(1 + true) }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.starts_with("error: type:"), "stderr: {stderr}");
}

#[test]
fn linearity_error() {
    let dir = std::env::temp_dir().join("slc_test_lin.sl");
    std::fs::write(&dir, "command bad(x: +i32) | (k: -i32) { x }").unwrap();
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
    std::fs::write(&dir, "command main | (exit: -i32) { println(add(1, 2)); 0 @ exit }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("3"));
}

#[test]
fn builtin_println() {
    let dir = std::env::temp_dir().join("slc_test_println.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { println(42); 0 @ exit }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn program_output_appears_in_order() {
    let dir = std::env::temp_dir().join("slc_test_println_vs_final_value.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { println(\"program output\"); println(42); 0 @ exit }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "\"program output\"\n42\n");
}

#[test]
fn a_program_prints_only_what_it_prints() {
    // The entry point is a command, so there is no final value to report:
    // output is exactly what the program printed.
    let dir = std::env::temp_dir().join("slc_test_no_final_value.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { println(42); 0 @ exit }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn lambda_application() {
    let dir = std::env::temp_dir().join("slc_test_lambda.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { println(fn(x: +i32) -> i32 { x }(5)); 0 @ exit }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn string_operations() {
    let dir = std::env::temp_dir().join("slc_test_str.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { println(str_concat(int_to_str(1), int_to_str(2))); 0 @ exit }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("12"));
}

#[test]
fn comparison() {
    let dir = std::env::temp_dir().join("slc_test_cmp.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { println(eq(1, 1)); 0 @ exit }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("true"));
}

#[test]
fn division_by_zero_rejected() {
    let dir = std::env::temp_dir().join("slc_test_div.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { println(div(1, 0)); 0 @ exit }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("division by zero"));
}

#[test]
fn parse_int_offers_a_parsed_value_to_its_ok_continuation() {
    let dir = std::env::temp_dir().join("slc_test_parse_ok.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) {
    let ok = fn(n: +i64) -> i32 { println(n); 1 @ exit };
    let invalid = fn(s: +String) -> i32 { println(s); println(2); 2 @ exit };
    let overflow = fn(s: +String) -> i32 { println(s); println(3); 3 @ exit };
    parse_int("42", ok, invalid, overflow)
}"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "stdout: {stdout}");
    assert!(stdout.contains("42"));
}

#[test]
fn parse_int_offers_an_invalid_input_to_its_failure_continuation() {
    let dir = std::env::temp_dir().join("slc_test_parse_empty.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) {
    let ok = fn(n: +i64) -> i32 { println(n); 1 @ exit };
    let invalid = fn(s: +String) -> i32 { println(s); println(2); 2 @ exit };
    let overflow = fn(s: +String) -> i32 { println(s); println(3); 3 @ exit };
    parse_int("", ok, invalid, overflow)
}"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "stdout: {stdout}");
    assert!(stdout.contains("2"));
}

#[test]
fn parse_int_offers_an_out_of_range_input_to_its_overflow_continuation() {
    let dir = std::env::temp_dir().join("slc_test_parse_overflow.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) {
    let ok = fn(n: +i64) -> i32 { println(n); 1 @ exit };
    let invalid = fn(s: +String) -> i32 { println(s); println(2); 2 @ exit };
    let overflow = fn(s: +String) -> i32 { println(s); println(3); 3 @ exit };
    parse_int("99999999999999999999999", ok, invalid, overflow)
}"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "stdout: {stdout}");
    assert!(stdout.contains("3"));
}

#[test]
fn short_circuit_and_does_not_evaluate_rhs() {
    let dir = std::env::temp_dir().join("slc_test_short_circuit.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { println(if false && (1 / 0 == 1) { 1 } else { 2 }); 0 @ exit }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("2"));
    assert!(!stderr.contains("division by zero"));
}

#[test]
fn short_circuit_or_does_not_evaluate_rhs() {
    let dir = std::env::temp_dir().join("slc_test_short_circuit_or.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { println(if true || (1 / 0 == 1) { 3 } else { 4 }); 0 @ exit }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("3"));
    assert!(!stderr.contains("division by zero"));
}

#[test]
fn subtraction_is_left_associative() {
    let dir = std::env::temp_dir().join("slc_test_assoc.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { println(10 - 3 - 2); 0 @ exit }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn boolean_precedence_below_comparisons() {
    let dir = std::env::temp_dir().join("slc_test_bool_prec.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { println(if 1 == 1 || 2 == 3 { 5 } else { 6 }); 0 @ exit }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn list_indexing_works() {
    let dir = std::env::temp_dir().join("slc_test_list_index.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { let xs = list_push(list_push(list_new(), 10), 20); println(xs[1]); 0 @ exit }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("20"));
}

#[test]
fn out_of_range_index_rejected() {
    let dir = std::env::temp_dir().join("slc_test_oob.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) { let xs = list_push(list_new(), 10); println(xs[5]); 0 @ exit }",
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("out of range"));
}

#[test]
fn slice_bounds_checked() {
    let dir = std::env::temp_dir().join("slc_test_slice_bounds.sl");
    std::fs::write(&dir, r#"command main | (exit: -i32) { println("abc"[1..9]); 0 @ exit }"#)
        .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("out of bounds"));
}

#[test]
fn named_error_propagation_success_path() {
    let dir = std::env::temp_dir().join("slc_test_named_error_ok.sl");
    std::fs::write(
        &dir,
        r#"command parse(input: +String) | (ok: -String, err: -String) {
            if input == "ok" { "parsed" @ ok } else { "failed" @ err }
        }
        command main | (exit: -i32) {
            let ok = fn(value: +String) -> i32 { println("ok: " + value); 0 @ exit };
            let err = fn(message: +String) -> i32 { println("err: " + message); 1 @ exit };
            parse("ok", ok, err)
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "expected the success continuation to exit 0, stderr: {stderr}");
    assert!(stdout.contains("ok: parsed"), "stdout: {stdout}");
}

#[test]
fn named_error_propagation_error_path() {
    let dir = std::env::temp_dir().join("slc_test_named_error_err.sl");
    std::fs::write(
        &dir,
        r#"command parse(input: +String) | (ok: -String, err: -String) {
            if input == "ok" { "parsed" @ ok } else { "failed" @ err }
        }
        command main | (exit: -i32) {
            let ok = fn(value: +String) -> i32 { println("ok: " + value); 0 @ exit };
            let err = fn(message: +String) -> i32 { println("err: " + message); 1 @ exit };
            parse("bad", ok, err)
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected the error continuation to exit 1, stderr: {stderr}");
    assert!(stdout.contains("err: failed"), "stdout: {stdout}");
}

#[test]
fn error_propagation_inside_nested_fn_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_nested_error_prop.sl");
    std::fs::write(
        &dir,
        r#"command parse(input: +String) | (ok: -String, err: -String) {
            if input == "ok" { "parsed" @ ok } else { "failed" @ err }
        }
        fn outer() -> i32 {
            let inner = fn(ignored: +i32) -> i32 { fail(ignored)? };
            inner(0)
        }
        command main | (exit: -i32) { 0 @ exit }"#,
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
        r#"command parse_json(input: +String) | (ok: -String, err: -String) {
            let start = skip_ws(input, 0);
            if start < str_len(input) {
                match input[start] {
                    '0'..='9' => input[start..start + 1] @ ok,
                    _ => "expected JSON value" @ err
                }
            } else {
                "empty input" @ err
            }
        }
        command main | (exit: -i32) {
            let ok = fn(value: +String) -> i32 { println("parsed: " + value); 0 @ exit };
            let err = fn(message: +String) -> i32 { println("error: " + message); 1 @ exit };
            parse_json("x", ok, err)
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "stderr: {stderr}");
    assert!(stdout.contains("error: expected JSON value"));
}

#[test]
fn select_dispatches_to_matching_enum_variant() {
    // Activating the constructed continuation with a variant runs that
    // variant's arm: `0 @ return` receives `Color::Red`, and so on.
    for (variant, expected) in [("Red", "0"), ("Green", "1"), ("Blue", "2")] {
        let dir = std::env::temp_dir().join(format!("slc_test_select_dispatch_{variant}.sl"));
        std::fs::write(
            &dir,
            format!(
                r#"enum Color {{ Red, Green, Blue }}
        fn k(return: -i32) <- Color {{
            select Color {{
                Red <= 0 @ return,
                Green <= 1 @ return,
                Blue <= 2 @ return,
            }}
        }}
        command main | (exit: -i32) {{
            println(mu run(out: -i32) {{
                Color::{variant} @ k(out)
            }});
            0 @ exit
        }}"#
            ),
        )
        .unwrap();
        let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
        assert!(ok, "{variant}: stderr: {stderr}");
        assert_eq!(stdout.trim(), expected, "{variant}: stdout: {stdout}");
    }
}

#[test]
fn activating_one_select_arm_does_not_activate_other_arms() {
    let dir = std::env::temp_dir().join("slc_test_select_activation_is_exclusive.sl");
    std::fs::write(
        &dir,
        r#"enum Color { Red, Green, Blue }

        fn shout(name: +String, code: +i32) -> i32 {
            println(name);
            code
        }

        fn dispatch(k: -i32) <- Color {
            select Color {
                Red <= shout("red", 0) @ k,
                Green <= shout("green", 1) @ k,
                Blue <= shout("blue", 2) @ k,
            }
        }

        command main | (exit: -i32) {
            println(mu run(out: -i32) {
                Color::Green @ dispatch(out)
            });
            0 @ exit
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("green"), "selected arm did not run: {stdout}");
    assert!(
        !stdout.contains("red") && !stdout.contains("blue"),
        "unselected arms must not run: {stdout}"
    );
    assert!(stdout.trim().ends_with('1'), "selected arm value: {stdout}");
}

#[test]
fn constructing_select_does_not_activate_any_arm() {
    // Every arm exits with a distinct nonzero status. Building the consumer
    // must not run any of them; only the activated one runs.
    let dir = std::env::temp_dir().join("slc_test_select_construction_is_lazy.sl");
    std::fs::write(
        &dir,
        r#"enum Color { Red, Green, Blue }

        fn boom(code: +i32) -> i32 {
            code @ exit
        }

        fn dispatch(k: -i32) <- Color {
            select Color {
                Red <= boom(3) @ k,
                Green <= boom(4) @ k,
                Blue <= boom(5) @ k,
            }
        }

        command main | (exit: -i32) {
            println(mu run(out: -i32) {
                let consumer = dispatch(out);
                7
            });
            0 @ exit
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "constructing `select` activated an arm: {stdout} {stderr}");
    assert_eq!(stdout.trim(), "7", "stdout: {stdout}");
}

#[test]
fn read_file_offers_a_missing_file_to_its_failure_continuation() {
    let dir = std::env::temp_dir().join("slc_test_read_missing.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) {
            read_file("does-not-exist.sl", fn(source: +String) -> ⊥ {
                println("unexpectedly read " + source);
                1 @ exit
            }, fn(message: +String) -> ⊥ {
                println("failed: " + message);
                0 @ exit
            })
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "a missing file is an outcome, not a fault: {stderr}");
    assert!(stdout.contains("failed: cannot read does-not-exist.sl"), "stdout: {stdout}");
}

#[test]
fn write_file_and_read_file_round_trip_through_their_continuations() {
    let target = std::env::temp_dir().join("slc_test_written.txt");
    let _ = std::fs::remove_file(&target);
    let dir = std::env::temp_dir().join("slc_test_write_read.sl");
    std::fs::write(
        &dir,
        format!(
            r#"command main | (exit: -i32) {{
            write_file({path:?}, "written", fn(done: +unit) -> ⊥ {{
                read_file({path:?}, fn(source: +String) -> ⊥ {{
                    println(source);
                    0 @ exit
                }}, fn(message: +String) -> ⊥ {{
                    println(message);
                    1 @ exit
                }})
            }}, fn(message: +String) -> ⊥ {{
                println(message);
                2 @ exit
            }})
        }}"#,
            path = target.to_str().unwrap()
        ),
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("written"), "stdout: {stdout}");
}

#[test]
fn lookup_builtins_offer_both_outcomes() {
    let dir = std::env::temp_dir().join("slc_test_lookup_outcomes.sl");
    std::fs::write(
        &dir,
        r#"fn report(message: +String) -> ⊥ {
            println(message);
            1 @ exit
        }

        command main | (exit: -i32) {
            let items = list_push(list_push(list_new(), 10), 20);
            list_get(items, 1, fn(second: +i64) -> ⊥ {
                println(second);
                list_get(items, 7, fn(unexpected: +i64) -> ⊥ {
                    report("unexpectedly found something")
                }, fn(message: +String) -> ⊥ {
                    println(message);
                    map_get(map_insert(map_new(), 1, "one"), 2, fn(found: +String) -> ⊥ {
                        report("unexpectedly found " + found)
                    }, fn(message: +String) -> ⊥ {
                        println(message);
                        0 @ exit
                    })
                })
            }, fn(message: +String) -> ⊥ { report(message) })
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("20"), "stdout: {stdout}");
    assert!(stdout.contains("index 7 is out of range for a list of length 2"), "stdout: {stdout}");
    assert!(stdout.contains("no entry for 2"), "stdout: {stdout}");
}

#[test]
fn select_builds_the_consumer_of_a_product() {
    // The negative multiplicative: one consumer with every component, which
    // must be supplied together.
    let dir = std::env::temp_dir().join("slc_test_select_product.sl");
    std::fs::write(
        &dir,
        r#"struct Reading { value: i64, unit: String }

        fn show(out: -String) <- Reading {
            select Reading {
                Reading { value, unit } <= (int_to_str(value) + unit) @ out,
            }
        }

        fn total(out: -i64) <- (+i64 ⊗ +i64) {
            select (+i64 ⊗ +i64) {
                (left, right) <= (left + right) @ out,
            }
        }

        command main | (exit: -i32) {
            println(mu sum(answer: -i64) { (2, 40) @ total(answer) });
            println(mu ask(answer: -String) {
                Reading { value: 42, unit: "m" } @ show(answer)
            });
            0 @ exit
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("42"), "stdout: {stdout}");
    assert!(stdout.contains("42m"), "stdout: {stdout}");
}

#[test]
fn a_struct_is_built_and_taken_apart_anywhere() {
    // A struct literal is an ordinary expression, and a struct pattern binds
    // its fields.
    let dir = std::env::temp_dir().join("slc_test_struct_roundtrip.sl");
    std::fs::write(
        &dir,
        r#"struct D { left: i64, right: i64 }

        fn sum(d: D) -> i64 {
            match d {
                D { left: a, right: b } => a + b,
                _ => 0,
            }
        }

        command main | (exit: -i32) {
            let d = D { left: 10, right: 20 };
            println(sum(d));
            0 @ exit
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "30", "stdout: {stdout}");
}

#[test]
fn control_does_not_return_from_a_cut() {
    // A cut is a command: the value goes to the consumer and the rest of the
    // block never runs.
    let dir = std::env::temp_dir().join("slc_test_cut_does_not_return.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) {
            println(mu run(k: -i64) {
                println("before");
                1 @ k;
                println("after")
            });
            0 @ exit
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("before"), "stdout: {stdout}");
    assert!(!stdout.contains("after"), "control returned from a cut: {stdout}");
    assert!(stdout.trim().ends_with('1'), "stdout: {stdout}");
}

#[test]
fn a_computed_consumer_receives_the_value() {
    // The right of `@` may be any expression that produces a consumer, not
    // only a name: here a negative function applied to a continuation.
    let dir = std::env::temp_dir().join("slc_test_cut_computed_consumer.sl");
    std::fs::write(
        &dir,
        r#"enum Color { Red, Green, Blue }

        fn code(return: -i64) <- Color {
            select Color {
                Red <= 0 @ return,
                Green <= 1 @ return,
                Blue <= 2 @ return,
            }
        }

        command main | (exit: -i32) {
            println(mu ask(answer: -i64) {
                Color::Blue @ code(answer)
            });
            0 @ exit
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "2", "stdout: {stdout}");
}

#[test]
fn calling_a_continuation_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_calling_a_continuation.sl");
    std::fs::write(
        &dir,
        "command bad(x: +i32) | (k: -i32) { k(x) }\ncommand main | (exit: -i32) { 0 @ exit }",
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected a diagnostic");
    assert!(stderr.contains("not a function"), "stderr: {stderr}");
    assert!(stderr.contains("value @ k"), "stderr: {stderr}");
}

#[test]
fn negative_fn_and_local_mu_capture_do_not_conflict() {
    let dir = std::env::temp_dir().join("slc_test_negative_fn_local_mu.sl");
    std::fs::write(
        &dir,
        r#"fn f(k: -i32) <- i32 {
            (mu escape(outer: -i32) { 42 @ outer }) @ k
        }
        command main | (exit: -i32) { 0 @ exit }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stdout={stdout:?}, stderr={stderr:?}");
}

/// The JSON example with its `source` binding replaced, so one program can be
/// pointed at any input.
fn json_parser_with_source(literal: &str, file: &str) -> (String, String, bool) {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/json_parser.sl"
    ))
    .unwrap();
    let start = source.find("let source = ").unwrap();
    let end = source[start..].find(";\n").unwrap() + start;
    let rewritten = format!("{}let source = {literal};{}", &source[..start], &source[end + 1..]);
    let path = std::env::temp_dir().join(file);
    std::fs::write(&path, rewritten).unwrap();
    run_sl(path.to_str().unwrap())
}

#[test]
fn json_parser_preserves_output_and_exit_status() {
    // The parser now reports through one `select`-built continuation instead
    // of a pair of continuations. Its output and exit status are unchanged.
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/json_parser.sl"
    ))
    .unwrap();
    let path = std::env::temp_dir().join("slc_test_json_unchanged.sl");
    std::fs::write(&path, source).unwrap();
    let (stdout, stderr, ok) = run_sl(path.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        "\"parsed: {\\\"name\\\":\\\"slant\\\",\\\"tags\\\":[1,2,-3.25],\\\"active\\\":true,\\\"none\\\":null,\\\"escaped\\\":\\\"a\\\\\\\"b\\\\u0041\\\"}\"",
        "stdout: {stdout}"
    );
}

#[test]
fn json_parser_accepts_nested_values_and_whitespace_boundaries() {
    for (name, literal) in [
        ("nested", r#""{\"a\":[{\"b\":[[1]]},2]}""#),
        ("whitespace", r#""  {\"a\" : [ 1 , 2 ] }  ""#),
        ("empty_containers", r#""[[],{}]""#),
        ("exponent", r#""[1e10,-2.5E-3]""#),
    ] {
        let (stdout, stderr, ok) =
            json_parser_with_source(literal, &format!("slc_test_json_ok_{name}.sl"));
        assert!(ok, "{name}: stderr: {stderr}");
        assert!(stdout.contains("parsed:"), "{name}: stdout: {stdout}");
    }
}

#[test]
fn json_parser_rejects_malformed_edge_cases() {
    for (name, literal, message) in [
        ("empty", r#""""#, "empty input"),
        ("value", r#""x""#, "expected JSON value"),
        ("unterminated_string", r#""\"abc""#, "unterminated JSON string"),
        ("bad_escape", r#""\"a\\qb\"""#, "invalid escape in JSON string"),
        ("bad_unicode_escape", r#""\"a\\u00zz\"""#, "invalid hexadecimal digit"),
        ("bad_literal", r#""tru""#, "invalid JSON literal"),
        ("missing_exponent", r#""1e""#, "expected digit in exponent"),
        ("missing_fraction", r#""1.""#, "expected digit after decimal point"),
        ("array_separator", r#""[1 2]""#, "expected `,` or `]` in array"),
        ("object_key", r#""{1:2}""#, "expected object key"),
        ("object_colon", r#""{\"a\" 1}""#, "expected `:` after object key"),
    ] {
        let (stdout, stderr, ok) =
            json_parser_with_source(literal, &format!("slc_test_json_bad_{name}.sl"));
        assert!(!ok, "{name}: expected failure, stdout: {stdout}, stderr: {stderr}");
        assert!(
            stdout.contains(message),
            "{name}: expected {message:?} in stdout: {stdout} (stderr: {stderr})"
        );
    }
}

#[test]
fn json_parser_rejects_malformed_input() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/json_parser.sl"
    ))
    .unwrap();
    let start = source.find("let source = ").unwrap();
    let end = source[start..].find(";\n").unwrap() + start;
    let malformed = format!("{}let source = \"{{\";{}", &source[..start], &source[end + 1..]);
    let dir = std::env::temp_dir().join("slc_test_json_malformed.sl");
    std::fs::write(&dir, malformed).unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected failure, stderr: {stderr}");
    assert!(
        stdout.contains("error:") || stderr.contains("error:"),
        "stdout: {stdout}; stderr: {stderr}"
    );
}

#[test]
fn a_file_handle_is_its_own_type_and_is_spent_by_close() {
    // An integer cannot close a file.
    let dir = std::env::temp_dir().join("slc_test_close_not_a_handle.sl");
    std::fs::write(&dir, "command main | (exit: -i32) { close_file(42); 0 @ exit }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(
        (stdout.clone() + &stderr).contains("expected +File"),
        "stdout: {stdout}; stderr: {stderr}"
    );

    // Reading through a closed handle fails.
    let data = std::env::temp_dir().join("slc_test_handle_data.txt");
    std::fs::write(&data, "one line\n").unwrap();
    let dir = std::env::temp_dir().join("slc_test_use_after_close.sl");
    std::fs::write(
        &dir,
        format!(
            r#"command main | (exit: -i32) {{
                let fail = select +String {{ m <= {{ println(m); 1 @ exit }} }};
                let fh = mu(k) {{ open_file("{}", k, fail) }};
                close_file(fh);
                let line = mu(k) {{
                    read_line(fh, k, select +unit {{ e <= {{ println("eof"); 1 @ exit }} }})
                }};
                println(line);
                0 @ exit
            }}"#,
            data.display()
        ),
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "stdout: {stdout}");
    assert!((stdout + &stderr).contains("is not open"), "a spent handle must not read: {stderr}");
}

#[test]
fn json_parser_rejects_trailing_characters() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/json_parser.sl"
    ))
    .unwrap();
    let start = source.find("let source = ").unwrap();
    let end = source[start..].find(";\n").unwrap() + start;
    let malformed = format!("{}let source = \"1 x\";{}", &source[..start], &source[end + 1..]);
    let dir = std::env::temp_dir().join("slc_test_json_trailing_char.sl");
    std::fs::write(&dir, malformed).unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stdout.contains("trailing characters"));
}

#[test]
fn json_parser_rejects_trailing_comma() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/json_parser.sl"
    ))
    .unwrap();
    let start = source.find("let source = ").unwrap();
    let end = source[start..].find(";\n").unwrap() + start;
    let malformed = format!("{}let source = \"[1,]\";{}", &source[..start], &source[end + 1..]);
    let dir = std::env::temp_dir().join("slc_test_json_trailing_comma.sl");
    std::fs::write(&dir, malformed).unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stdout.contains("trailing comma"));
}

#[test]
fn a_consumer_built_over_an_atom_receives_the_value() {
    // `select` covers every positive type, atoms included, so a consumer can
    // be bound to a name and cut against later.
    let dir = std::env::temp_dir().join("slc_test_select_atom.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) {
             let show = select +i64 { n <= println(n * 2) };
             21 @ show;
             0 @ exit
         }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}
