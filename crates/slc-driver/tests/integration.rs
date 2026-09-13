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
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(42); 0 | exit⟩ }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn exit_zero_returns_success() {
    let dir = std::env::temp_dir().join("slc_test_exit_zero.sl");
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { 0 | exit⟩ }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "expected `0 | EXIT` to succeed⟩, stderr: {stderr}");
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
    std::fs::write(&dir, "fn main(k: -i32) <- i32 { 0 | k⟩ }").unwrap();
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
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(7); 0 | exit⟩ }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("7"), "stdout: {stdout}");

    // A value parameter, or a row that is not one exit status, is rejected.
    for rejected in [
        "command main(x: +i32) | (exit: -i32) { 0 | exit⟩ }",
        "command main | (a: -i32 & b: -i32) { if true { 0 | a⟩ } else { 1 | b⟩ } }",
        "command main | (exit: -String) { \"done\" | exit⟩ }",
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
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { 7 | exit⟩ }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected `7 | EXIT` to fail");
    assert!(!stderr.contains("exit(7)"), "EXIT must not be reported as a diagnostic: {stderr}");
}

#[test]
fn diagnostic_that_merely_looks_like_exit_is_not_treated_as_exit() {
    let dir = std::env::temp_dir().join("slc_test_exit_like_diagnostic.sl");
    std::fs::write(
        &dir,
        r#"fn missing(value: +String, exit: -i32) -> i32 { 0 | exit⟩ }
        command main | (exit: -i32) / {IO} { println(exit_like); 0 | exit⟩ }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("`exit_like` is not defined"), "stderr: {stderr}");
    assert!(!stderr.contains("exit("), "diagnostic must not be parsed as EXIT: {stderr}");
}

#[test]
fn polarity_error() {
    // A consumer value parameter is fine now; a positive continuation
    // parameter is the polarity error that remains.
    let dir = std::env::temp_dir().join("slc_test_pol.sl");
    std::fs::write(&dir, "command bad | (j: +i32 & k: -i32) { 0 | k⟩ }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("polarity"));
}

#[test]
fn checker_diagnostics_include_source_locations() {
    let cases = [
        (
            "slc_test_location_type.sl",
            "command main | (exit: -i32) / {IO} { println(1 + true); 0 | exit⟩ }",
            ["type:", "1:46", "`1 + true`"],
        ),
        (
            "slc_test_location_polarity.sl",
            "command bad | (j: +i32 & k: -i32) { 0 | k⟩ }",
            ["polarity:", "1:1", "`command`"],
        ),
        (
            "slc_test_location_bottom.sl",
            "command bad(x: +i32) | (k: -i32) { x }",
            ["type:", "1:34", "`{ x }`"],
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
    std::fs::write(&dir, "command main( | (exit: -i32) { 0 | exit⟩ }").unwrap();
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
fn a_command_body_must_reach_a_continuation() {
    // A command whose body is a bare value reaches no continuation: its body
    // is not `⊥`, so it is rejected by the type checker.
    let dir = std::env::temp_dir().join("slc_test_bottom.sl");
    std::fs::write(&dir, "command bad(x: +i32) | (k: -i32) { x }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("must reach a continuation"), "stderr: {stderr}");
}

#[test]
fn no_input_file() {
    let out = Command::new(env!("CARGO_BIN_EXE_slc")).output().expect("failed to run slc");
    assert!(!out.status.success());
}

#[test]
fn builtin_add() {
    let dir = std::env::temp_dir().join("slc_test_add.sl");
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(add(1, 2)); 0 | exit⟩ }")
        .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("3"));
}

#[test]
fn builtin_println() {
    let dir = std::env::temp_dir().join("slc_test_println.sl");
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(42); 0 | exit⟩ }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn program_output_appears_in_order() {
    let dir = std::env::temp_dir().join("slc_test_println_vs_final_value.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) / {IO} { println(\"program output\"); println(42); 0 | exit⟩ }",
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
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(42); 0 | exit⟩ }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn lambda_application() {
    let dir = std::env::temp_dir().join("slc_test_lambda.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) / {IO} { println(fn(x: +i32) -> i32 { x }(5)); 0 | exit⟩ }",
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
        "command main | (exit: -i32) / {IO} { println(str_concat(int_to_str(1), int_to_str(2))); 0 | exit⟩ }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("12"));
}

#[test]
fn comparison() {
    let dir = std::env::temp_dir().join("slc_test_cmp.sl");
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(eq(1, 1)); 0 | exit⟩ }")
        .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("true"));
}

#[test]
fn division_by_zero_rejected() {
    let dir = std::env::temp_dir().join("slc_test_div.sl");
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(div(1, 0)); 0 | exit⟩ }")
        .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("division by zero"));
}

#[test]
fn parse_int_offers_a_parsed_value_to_its_ok_continuation() {
    let dir = std::env::temp_dir().join("slc_test_parse_ok.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) / {IO} {
    let ok = fn(n: +i64) -> i32 { println(n); 1 | exit⟩ };
    let invalid = fn(s: +String) -> i32 { println(s); println(2); 2 | exit⟩ };
    let overflow = fn(s: +String) -> i32 { println(s); println(3); 3 | exit⟩ };
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
        r#"command main | (exit: -i32) / {IO} {
    let ok = fn(n: +i64) -> i32 { println(n); 1 | exit⟩ };
    let invalid = fn(s: +String) -> i32 { println(s); println(2); 2 | exit⟩ };
    let overflow = fn(s: +String) -> i32 { println(s); println(3); 3 | exit⟩ };
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
        r#"command main | (exit: -i32) / {IO} {
    let ok = fn(n: +i64) -> i32 { println(n); 1 | exit⟩ };
    let invalid = fn(s: +String) -> i32 { println(s); println(2); 2 | exit⟩ };
    let overflow = fn(s: +String) -> i32 { println(s); println(3); 3 | exit⟩ };
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
        "command main | (exit: -i32) / {IO} { println(if false && (1 / 0 == 1) { 1 } else { 2 }); 0 | exit⟩ }",
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
        "command main | (exit: -i32) / {IO} { println(if true || (1 / 0 == 1) { 3 } else { 4 }); 0 | exit⟩ }",
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
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { println(10 - 3 - 2); 0 | exit⟩ }")
        .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn boolean_precedence_below_comparisons() {
    let dir = std::env::temp_dir().join("slc_test_bool_prec.sl");
    std::fs::write(
        &dir,
        "command main | (exit: -i32) / {IO} { println(if 1 == 1 || 2 == 3 { 5 } else { 6 }); 0 | exit⟩ }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn out_of_range_index_rejected() {
    let dir = std::env::temp_dir().join("slc_test_oob.sl");
    std::fs::write(&dir, r#"command main | (exit: -i32) / {IO} { println("ab"[5]); 0 | exit⟩ }"#)
        .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("out of range"));
}

#[test]
fn slice_bounds_checked() {
    let dir = std::env::temp_dir().join("slc_test_slice_bounds.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) / {IO} { println("abc"[1..9]); 0 | exit⟩ }"#,
    )
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
        r#"command parse(input: +String) | (ok: -String & err: -String) {
            if input == "ok" { "parsed" | ok⟩ } else { "failed" | err⟩ }
        }
        command main | (exit: -i32) / {IO} {
            let ok = fn(value: +String) -> i32 { println("ok: " + value); 0 | exit⟩ };
            let err = fn(message: +String) -> i32 { println("err: " + message); 1 | exit⟩ };
            "ok" | parse | (ok & err)⟩
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
        r#"command parse(input: +String) | (ok: -String & err: -String) {
            if input == "ok" { "parsed" | ok⟩ } else { "failed" | err⟩ }
        }
        command main | (exit: -i32) / {IO} {
            let ok = fn(value: +String) -> i32 { println("ok: " + value); 0 | exit⟩ };
            let err = fn(message: +String) -> i32 { println("err: " + message); 1 | exit⟩ };
            "bad" | parse | (ok & err)⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected the error continuation to exit 1, stderr: {stderr}");
    assert!(stdout.contains("err: failed"), "stdout: {stdout}");
}

#[test]
fn json_selected_error_continuation_reports_parse_error() {
    let dir = std::env::temp_dir().join("slc_test_json_selected_error.sl");
    std::fs::write(
        &dir,
        r#"command parse_json(input: +String) | (ok: -String & err: -String) {
            let start = (input, 0) | skip_ws;
            if start < (input | str_len) {
                match input[start] {
                    '0'..='9' => input[start..start + 1] | ok⟩,
                    _ => "expected JSON value" | err⟩
                }
            } else {
                "empty input" | err⟩
            }
        }
        command main | (exit: -i32) / {IO} {
            let ok = fn(value: +String) -> i32 { println("parsed: " + value); 0 | exit⟩ };
            let err = fn(message: +String) -> i32 { println("error: " + message); 1 | exit⟩ };
            "x" | parse_json | (ok & err)⟩
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
    // variant's arm: `0 | return` receives `Color::Red`, and so on.
    for (variant, expected) in [("Red", "0"), ("Green", "1"), ("Blue", "2")] {
        let dir = std::env::temp_dir().join(format!("slc_test_select_dispatch_{variant}.sl"));
        std::fs::write(
            &dir,
            format!(
                r#"enum Color {{ Red, Green, Blue }}
        fn k(return: -i32) <- Color {{
            select Color {{
                Red => 0 | return⟩,
                Green => 1 | return⟩,
                Blue => 2 | return⟩,
            }}
        }}
        command main | (exit: -i32) / {{IO}} {{
            mu i32 {{ out <= Color::{variant} | k | out⟩ }} | println;
            0 | exit⟩
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

        fn shout(name: +String, code: +i32) -> i32 / {IO} {
            name | println;
            code
        }

        fn dispatch(k: i32) <- Color / {IO} {
            select Color {
                Red => ("red", 0) | shout | k⟩,
                Green => ("green", 1) | shout | k⟩,
                Blue => ("blue", 2) | shout | k⟩,
            }
        }

        command main | (exit: -i32) / {IO} {
            mu i32 { out <= Color::Green | dispatch | out⟩ } | println;
            0 | exit⟩
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

        fn boom(code: +i32) -> i32 / {IO} {
            "BOOM" | println;
            code
        }

        fn dispatch(k: i32) <- Color / {IO} {
            select Color {
                Red => 3 | boom | k⟩,
                Green => 4 | boom | k⟩,
                Blue => 5 | boom | k⟩,
            }
        }

        command main | (exit: -i32) / {IO} {
            mu i32 { out <= {
                let consumer = out | dispatch;
                7
            } } | println;
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "constructing `select` activated an arm: {stdout} {stderr}");
    assert_eq!(stdout.trim(), "7", "stdout: {stdout}");
}

#[test]
fn fs_read_offers_a_missing_file_to_its_failure_continuation() {
    let dir = std::env::temp_dir().join("slc_test_read_missing.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) / {IO} {
            "does-not-exist.sl" | fs::read | (select String {
                source => { println("unexpectedly read " + source); 1 | exit⟩ },
            } & select String {
                message => { println("failed: " + message); 0 | exit⟩ },
            })⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "a missing file is an outcome, not a fault: {stderr}");
    assert!(stdout.contains("failed: cannot read does-not-exist.sl"), "stdout: {stdout}");
}

#[test]
fn fs_write_and_read_round_trip_through_their_continuations() {
    let target = std::env::temp_dir().join("slc_test_written.txt");
    let _ = std::fs::remove_file(&target);
    let dir = std::env::temp_dir().join("slc_test_write_read.sl");
    std::fs::write(
        &dir,
        format!(
            r#"command main | (exit: -i32) / {{IO}} {{
            let failed = select String {{ message => {{ println(message); 1 | exit⟩ }} }};
            ({path:?}, "written") | fs::write | (select unit {{
                done => {path:?} | fs::read | (select String {{
                    source => {{ println(source); 0 | exit⟩ }},
                }} & failed)⟩,
            }} & failed)⟩
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
        r#"fn report(message: +String, exit: -i32) -> ⊥ / {IO} {
            println(message);
            1 | exit⟩
        }

        command main | (exit: -i32) / {IO} {
            char_at("slant", 1, fn(second: +char) -> ⊥ {
                println(second);
                char_at("slant", 9, fn(unexpected: +char) -> ⊥ {
                    (("unexpectedly found something", exit) | report)
                }, fn(message: +String) -> ⊥ {
                    println(message);
                    0 | exit⟩
                })
            }, fn(message: +String) -> ⊥ { ((message, exit) | report) })
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("'l'"), "stdout: {stdout}");
    assert!(stdout.contains("out of range"), "stdout: {stdout}");
}

#[test]
fn select_builds_the_consumer_of_a_product() {
    // The negative multiplicative: one consumer with every component, which
    // must be supplied together.
    let dir = std::env::temp_dir().join("slc_test_select_product.sl");
    std::fs::write(
        &dir,
        r#"data Reading { value: i64, unit: String }

        fn show(out: -String) <- Reading {
            select Reading {
                Reading { value, unit } => ((value | int_to_str) + unit) | out⟩,
            }
        }

        fn total(out: -i64) <- (+i64 ⊗ +i64) {
            select (+i64 ⊗ +i64) {
                (left, right) => (left + right) | out⟩,
            }
        }

        command main | (exit: -i32) / {IO} {
            mu i64 { answer <= (2, 40) | (answer | total)⟩ } | println;
            mu String { answer <= Reading { value: 42, unit: "m" } | show | answer⟩ } | println;
            0 | exit⟩
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
    // A record literal is an ordinary expression, and a record pattern binds
    // its fields.
    let dir = std::env::temp_dir().join("slc_test_struct_roundtrip.sl");
    std::fs::write(
        &dir,
        r#"data D { left: i64, right: i64 }

        fn sum(d: D) -> i64 {
            match d {
                D { left: a, right: b } => a + b,
                _ => 0,
            }
        }

        command main | (exit: -i32) / {IO} {
            let d = D { left: 10, right: 20 };
            println((d | sum));
            0 | exit⟩
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
        r#"command main | (exit: -i32) / {IO} {
            println(mu i64 { k <= {
                println("before");
                1 | k⟩;
                println("after")
            } });
            0 | exit⟩
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
                Red => 0 | return⟩,
                Green => 1 | return⟩,
                Blue => 2 | return⟩,
            }
        }

        command main | (exit: -i32) / {IO} {
            mu i64 { answer <= Color::Blue | code | answer⟩ } | println;
            0 | exit⟩
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
        "command bad(x: +i32) | (k: -i32) { k(x) }\ncommand main | (exit: -i32) / {IO} { 0 | exit⟩ }",
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected a diagnostic");
    assert!(stderr.contains("not a function"), "stderr: {stderr}");
    assert!(stderr.contains("value | k"), "stderr: {stderr}");
}

#[test]
fn negative_fn_and_local_mu_capture_do_not_conflict() {
    let dir = std::env::temp_dir().join("slc_test_negative_fn_local_mu.sl");
    std::fs::write(
        &dir,
        r#"fn f(k: -i32) <- i32 {
            (mu i32 { outer <= 42 | outer⟩ }) | k⟩
        }
        command main | (exit: -i32) / {IO} { 0 | exit⟩ }"#,
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
    std::fs::write(&dir, "command main | (exit: -i32) / {IO} { 42 | fs::close; 0 | exit⟩ }")
        .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!((stdout.clone() + &stderr).contains("File"), "stdout: {stdout}; stderr: {stderr}");

    // Reading through a closed handle fails.
    let data = std::env::temp_dir().join("slc_test_handle_data.txt");
    std::fs::write(&data, "one line\n").unwrap();
    let dir = std::env::temp_dir().join("slc_test_use_after_close.sl");
    std::fs::write(
        &dir,
        format!(
            r#"command main | (exit: -i32) / {{IO}} {{
                let fail = select String {{ m => {{ println(m); 1 | exit⟩ }} }};
                let fh = mu {{ k <= "{}" | fs::open | (k & fail)⟩ }};
                fh | fs::close;
                let line = mu {{ k <=
                    fh | fs::read_line | (k & select unit {{ e => {{ println("eof"); 1 | exit⟩ }} }})⟩
                }};
                println(line);
                0 | exit⟩
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
fn the_prelude_is_available_and_shadowable() {
    // Prelude declarations are in scope without an import: `Display` and
    // `to_string`, and `IO`.
    let dir = std::env::temp_dir().join("slc_test_prelude.sl");
    std::fs::write(
        &dir,
        r#"fn double(n: +i64) -> i64 { n * 2 }
        command main | (exit: -i32) / {IO} {
            println((21 | double | to_string));
            println((true | fmt));
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["\"42\"", "\"true\""]);

    // A program's own definition shadows the prelude's.
    let dir = std::env::temp_dir().join("slc_test_prelude_shadow.sl");
    std::fs::write(
        &dir,
        r#"fn to_string(n: +i64) -> String { "mine" }
        command main | (exit: -i32) / {IO} { println((7 | to_string)); 0 | exit⟩ }"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert_eq!(stdout.trim(), "\"mine\"");

    // The program's text precedes the prelude, so a diagnostic keeps the
    // program's own line and column.
    let dir = std::env::temp_dir().join("slc_test_prelude_spans.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) / {IO} {
    println(1 + "x");
    0 | exit⟩
}"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("at 2:"), "the program's own position: {stderr}");
}

#[test]
fn a_multi_parameter_function_travels_as_a_value() {
    // Its parameters pack into one product, so naming it and calling it
    // through a binding or a higher-order parameter all agree.
    let dir = std::env::temp_dir().join("slc_test_packed_hof.sl");
    std::fs::write(
        &dir,
        r#"fn plus(a: i64, b: i64) -> i64 { a + b }
        fn apply2(f: ((+i64 ⊗ +i64) -> +i64), x: i64, y: i64) -> i64 { f(x, y) }
        command main | (exit: -i32) / {IO} {
            println(((1, 2) | plus));
            let g = plus;
            println(g(10, 20));
            println(((plus, 3, 4) | apply2));
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["3", "30", "7"]);
}

#[test]
fn a_function_headed_chain_composes_unless_marked() {
    // A head that is not a function applies; a head that is one composes,
    // and `⟨` says it is the value flowing in instead.
    let dir = std::env::temp_dir().join("slc_test_flow_head.sl");
    std::fs::write(
        &dir,
        r#"fn double(n: i64) -> i64 { n * 2 }
        fn describe(out: -String) <- ((+i64 -> +i64)) {
            fn(f: (+i64 -> +i64)) { "a function" | out⟩ }
        }
        command main | (exit: -i32) / {IO} {
            21 | double | println;             // a value heads it: apply
            let quadruple = double | double;   // a function heads it: compose
            5 | quadruple | println;
            mu String { s <= ⟨double | describe | s⟩ } | println;
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["42", "20", "\"a function\""]);
}

#[test]
fn a_row_closes_the_chain_and_travels_whole() {
    // A command's exits are the chain's closing stage, written as one
    // bundle — and a row is first class: a command can take one and hand
    // it on unopened.
    let dir = std::env::temp_dir().join("slc_test_row_forms.sl");
    std::fs::write(
        &dir,
        r#"command classify(n: i64) | (found: i64 & missing: String) {
            if n > 0 { n | found⟩ } else { "negative" | missing⟩ }
        }
        command forward(n: i64) | (row: (-i64 & -String)) { n | classify | row⟩ }
        command main | (exit: -i32) / {IO} {
            mu i64 { ok <= 7 | classify | (ok & select +String { s => s | str_len | ok⟩ })⟩ } | println;
            mu i64 { ok <= 0 - 1 | forward | (ok & select +String { s => s | str_len | ok⟩ })⟩ } | println;
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["7", "8"]);
}

#[test]
fn an_operation_may_take_several_parameters() {
    // Calls are curried, so an operation of several parameters collects
    // them before performing — otherwise it would perform on its first
    // argument and apply the rest to the handler's answer.
    let dir = std::env::temp_dir().join("slc_test_op_arity.sl");
    std::fs::write(
        &dir,
        r#"effect Tag { fn tag(label: +String, n: +i64) -> i64; }
        command main | (exit: -i32) / {IO} {
            handle (("ten", 7) | tag) { tag(l, n): k => (n * 10 | k), return(m) => m } | println;
            handle (("len", 7) | tag) { tag(l, n) => (l | str_len) + n, return(m) => m } | println;
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["70", "10"]);
}

#[test]
fn the_prelude_provides_all_four_logical_units() {
    let dir = std::env::temp_dir().join("slc_test_prelude_units.sl");
    std::fs::write(
        &dir,
        r#"fn unit_value() -> Unit { (,) }
        fn builtin_unit_value() -> unit { Unit {} }
        fn top_value() -> Top { mu Top {} }
        fn use_empty<T>(empty: Empty) -> T { match empty {} }
        fn use_bottom<T>(bottom: Bottom) -> T {
            Bottom {} | bottom⟩
        }
        command bottom_command | (exit: -i32) -> Bottom { 0 | exit⟩ }
        command main | (exit: -i32) / {IO} {
            match unit_value() { Unit {} => println("unit") };
            match builtin_unit_value() { (,) => println("unit again") };
            println(mu i64 {
                out <= (select Bottom { Bottom {} => 42 | out⟩ } | use_bottom) | out⟩
            });
            println(mu i64 {
                out <= Bottom {} | select Bottom { (,) => 43 | out⟩ }⟩
            });
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["\"unit\"", "\"unit again\"", "42", "43"]);
}

#[test]
fn a_stdlib_module_is_reached_by_path_or_use_and_not_otherwise() {
    let dir = std::env::temp_dir().join("slc_test_stdlib_reach.sl");
    std::fs::write(
        &dir,
        r#"use num::max;
        command main | (exit: -i32) / {IO} {
            println(((3, 7) | num::min));
            println(((3, 7) | max));
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["3", "7"]);

    // Unimported, a stdlib name is not in scope.
    let dir = std::env::temp_dir().join("slc_test_stdlib_unreached.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: -i32) / {IO} { println(((3, 7) | min)); 0 | exit⟩ }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("min"), "stderr: {stderr}");

    // A program's own module shadows a stdlib module of the same name, whole.
    let dir = std::env::temp_dir().join("slc_test_stdlib_shadow_mod.sl");
    std::fs::write(
        &dir,
        r#"mod num { pub fn min(a: +i64, b: +i64) -> i64 { a + 100 } }
        command main | (exit: -i32) / {IO} { println(((3, 7) | num::min)); 0 | exit⟩ }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "103");

    // A library-side diagnostic names its unit.
    let dir = std::env::temp_dir().join("slc_test_stdlib_diag.sl");
    std::fs::write(
        &dir,
        r#"use list::List::*;
        command main | (exit: -i32) / {IO} { (Nil, "x") | list::nth | (exit & exit)⟩ }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(!stderr.contains(".sl:"), "an error in the program's own text names no unit: {stderr}");
}

#[test]
fn the_prelude_tap_is_a_command_and_composes_with_builtins() {
    let dir = std::env::temp_dir().join("slc_test_prelude_combinators.sl");
    std::fs::write(
        &dir,
        r#"command main | (exit: i32) / {IO} {
            mu i64 { out <= ("answer", 42) | trace::tap | out⟩ } | println;
            // A row slot wants a consumer, and `select` is what builds one.
            mu i64 { out <=
                "nope" | parse_int | (out
                    & select String { m => 7 | out⟩ }
                    & select String { m => 9 | out⟩ })⟩
            } | println;
            mu i64 { out <=
                "35" | parse_int | (out
                    & select String { m => 7 | out⟩ }
                    & select String { m => 9 | out⟩ })⟩
            } | println;
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(
        stdout.split_whitespace().collect::<Vec<_>>(),
        ["\"answer\"", "42", "42", "7", "35"]
    );
}

#[test]
fn display_formats_through_bounded_impls() {
    let dir = std::env::temp_dir().join("slc_test_display.sl");
    std::fs::write(
        &dir,
        r#"use list::List;
        command main | (exit: -i32) / {IO} {
            println(fmt(42));
            println(fmt("plain"));
            println(fmt(false));
            println((7 | to_string));
            let xs = List::Cons(1, List::Cons(2, List::Nil));
            println(fmt(xs));
            println((xs | to_string));
            println(fmt(List::Cons(xs, List::Cons(List::Nil, List::Nil))));
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "\"42\"",
            "\"plain\"",
            "\"false\"",
            "\"7\"",
            "\"[1, 2]\"",
            "\"[1, 2]\"",
            "\"[[1, 2], []]\""
        ],
        "stdout: {stdout}"
    );

    // A type without an impl is rejected, not garbled.
    let dir = std::env::temp_dir().join("slc_test_display_missing.sl");
    std::fs::write(
        &dir,
        r#"data P { x: i64 }
        command main | (exit: -i32) / {IO} { println(fmt(P { x: 1 })); 0 | exit⟩ }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("no `impl Display for P`"), "stderr: {stderr}");
}

#[test]
fn variant_imports_pin_bare_names_and_ambiguity_is_an_error() {
    // A glob import pins the bare names even beside a competing enum.
    let dir = std::env::temp_dir().join("slc_test_use_glob.sl");
    std::fs::write(
        &dir,
        r#"use list::List;
        use list::List::*;
        enum Mine { Nil, Cons(i64, Mine) }
        fn total(xs: List<i64>) -> i64 {
            match xs { Nil => 0, Cons(n, rest) => n + (rest | total) }
        }
        command main | (exit: -i32) / {IO} { Cons(40, Cons(2, Nil)) | total | println; 0 | exit⟩ }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");

    // Without an import, an ambiguous bare pattern is an error — never a
    // silent catch-all binder.
    let dir = std::env::temp_dir().join("slc_test_use_ambiguous.sl");
    std::fs::write(
        &dir,
        r#"enum Mine { Nil, Cons(i64, Mine) }
        fn count(xs: Mine) -> i64 {
            match xs { Nil => 0, Cons(_, rest) => 1 + count(rest) }
        }
        command main | (exit: -i32) / {IO} { println(count(Mine::Nil)); 0 | exit⟩ }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("variant of more than one enum"), "stderr: {stderr}");

    // Colliding imports are an error at the `use`.
    let dir = std::env::temp_dir().join("slc_test_use_collision.sl");
    std::fs::write(
        &dir,
        r#"use list::List::*;
        enum Mine { Nil }
        use Mine::*;
        command main | (exit: -i32) / {IO} { 0 | exit⟩ }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("imported from both"), "stderr: {stderr}");
}

#[test]
fn traits_dispatch_on_menu_and_form_receivers() {
    // Codata carries impls: a concrete menu, a form, a bounded impl for a
    // generic menu, and a bound discharged at a menu type all dispatch.
    let dir = std::env::temp_dir().join("slc_test_trait_menu.sl");
    std::fs::write(
        &dir,
        r#"menu Config { retries: i64, name: String }
        form Report { value: i32, label: String }
        menu Stream2<T> { head: T, tail: Stream2<T> }

        trait Describe { fn describe(self: +Self) -> String; }

        impl Describe for Config {
            fn describe(self: +Config) -> String {
                self.name + " with " + fmt(self.retries) + " retries"
            }
        }
        impl Describe for Report {
            fn describe(self: +Report) -> String { "a report sink" }
        }
        impl<T: Display> Describe for Stream2<T> {
            fn describe(self: +Stream2<T>) -> String {
                "stream starting " + fmt(self.head)
            }
        }

        fn config() -> Config {
            mu Config { retries <= 3 | retries⟩, name <= "slant" | name⟩ }
        }
        fn printer(out: -i32) -> Report {
            select Report { Report { value, label } => value | out⟩ }
        }
        fn ones() -> Stream2<i64> {
            mu Stream2 { head: out <= 1 | out⟩, tail: out <= ones() | out⟩ }
        }
        fn label<T: Describe>(x: T) -> String { describe(x) }

        command main | (exit: -i32) / {IO} {
            println(describe(config()));
            println(describe((exit | printer)));
            println(describe(ones()));
            config() | label | println;
            (exit | printer) | label | println;
            0 | exit⟩
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "\"slant with 3 retries\"",
            "\"a report sink\"",
            "\"stream starting 1\"",
            "\"slant with 3 retries\"",
            "\"a report sink\"",
        ],
        "stdout: {stdout}"
    );
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
        "command main | (exit: -i32) / {IO} {
             let show = select +i64 { n => println(n * 2) };
             21 | show⟩;
             0 | exit⟩
         }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}
