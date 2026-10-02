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
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <42 | println; <0 | exit> }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn exit_zero_returns_success() {
    let dir = std::env::temp_dir().join("slc_test_exit_zero.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <0 | exit> }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "expected `0 | EXIT>` to succeed, stderr: {stderr}");
}

#[test]
fn missing_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_missing_main.sl");
    std::fs::write(&dir, "func helper() -> i32 { 0 }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("no `main`"), "stderr: {stderr}");
}

#[test]
fn malformed_negative_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_negative_main.sl");
    std::fs::write(&dir, "func main(k: -i32) <- i32 { <0 | k> }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("entry point must be"), "stderr: {stderr}");
}

#[test]
fn malformed_parameterized_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_parameterized_main.sl");
    std::fs::write(&dir, "func main(x: +i32) -> i32 { x }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("entry point must be"), "stderr: {stderr}");
}

#[test]
fn malformed_generic_main_is_rejected() {
    let dir = std::env::temp_dir().join("slc_test_generic_main.sl");
    std::fs::write(&dir, "func main<+T>() -> i32 { 0 }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("entry point must be"), "stderr: {stderr}");
}

#[test]
fn the_accepted_entry_point_is_a_command_with_one_exit_continuation() {
    let dir = std::env::temp_dir().join("slc_test_valid_main.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <7 | println; <0 | exit> }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("7"), "stdout: {stdout}");

    // A value parameter, or a row that is not one exit status, is rejected.
    for rejected in [
        "proc main(x: +i32) | (exit: -i32) { <0 | exit> }",
        "proc main | (a: -i32 & b: -i32) { of True { True => <0 | a>, _ => <1 | b> } }",
        "proc main | (exit: -String) { <\"done\" | exit> }",
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
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <7 | exit> }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "expected `7 | EXIT` to fail");
    assert!(!stderr.contains("exit(7)"), "EXIT must not be reported as a diagnostic: {stderr}");
}

#[test]
fn diagnostic_that_merely_looks_like_exit_is_not_treated_as_exit() {
    let dir = std::env::temp_dir().join("slc_test_exit_like_diagnostic.sl");
    std::fs::write(
        &dir,
        r#"func missing(value: +String, exit: -i32) -> i32 { <0 | exit> }
        proc main | (exit: -i32) / {IO} { <exit_like | println; <0 | exit> }"#,
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
    std::fs::write(&dir, "proc bad | (j: +i32 & k: -i32) { <0 | k> }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("polarity"));
}

#[test]
fn checker_diagnostics_include_source_locations() {
    let cases = [
        (
            "slc_test_location_type.sl",
            "proc main | (exit: -i32) / {IO} { <(1, True) | add | println; <0 | exit> }",
            ["type:", "1:48", "`add`"],
        ),
        (
            "slc_test_location_polarity.sl",
            "proc bad | (j: +i32 & k: -i32) { <0 | k> }",
            ["polarity:", "1:1", "`proc`"],
        ),
        (
            "slc_test_location_bottom.sl",
            "proc bad(x: +i32) | (k: -i32) { x }",
            ["type:", "1:31", "`{ x }`"],
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
    std::fs::write(&dir, "proc main( | (exit: -i32) { 0 | exit> }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.starts_with("error: parse error:"), "stderr: {stderr}");

    // A valid parse with a type error must stop before polarity checking.
    let dir = std::env::temp_dir().join("slc_test_phase_type_precedence.sl");
    std::fs::write(&dir, "proc bad(x: -i32) | (k: -i32) { k((<(1, True) | add)) }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.starts_with("error: type:"), "stderr: {stderr}");
}

#[test]
fn a_command_body_must_reach_a_continuation() {
    // A command whose body is a bare value reaches no continuation: its body
    // is not `(;)`, so it is rejected by the type checker.
    let dir = std::env::temp_dir().join("slc_test_bottom.sl");
    std::fs::write(&dir, "proc bad(x: +i32) | (k: -i32) { x }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("must reach a continuation"), "stderr: {stderr}");
}

#[test]
fn no_input_file() {
    let out = Command::new(env!("CARGO_BIN_EXE_slc")).output().expect("failed to run slc");
    assert!(!out.status.success());
}

fn run_sl_with(args: &[&str], name: &str, source: &str) -> (String, String, bool) {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .args(args)
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.success(),
    )
}

const LONG_LOOP: &str = r#"func count(n: i64, acc: i64) -> i64 {
    of (<(n, 0) | eq) {
        True => acc,
        False => <((<(n, 1) | sub), (<(acc, 1) | add)) | count,
    }
}

proc main | (exit: -i32) / {IO} {
    <(100000, 0) | count | println;
    <0 | exit>
}"#;

const DELAYED_DECLS: &str = r#"hook Exn { func throw(m: String) -> i64; }

func make() -> (i64 -> i64) / {Exn} {
    <"made" | throw;
    fn(n: i64) { <(n, 1) | add }
}

func apply5(f: (i64 -> i64)) -> i64 { <5 | f }
func apply5_effectful(f: (-> (i64 -> i64) / {Exn})) -> i64 / {Exn} { <5 | f }

enum Held { Holds((i64 -> i64)) }
"#;

fn delayed_program(body: &str) -> String {
    format!("{DELAYED_DECLS}\nproc main | (exit: -i32) / {{IO}} {{\n{body}\n<0 | exit>\n}}\n")
}

#[test]
fn a_delayed_computation_performs_under_the_handler_around_its_use() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_delayed_accepted.sl",
        &delayed_program(
            r#"// Handled around the use.
            let- f = make();
            let a = do <5 | f hn { throw(m) => -1, };
            <a | println;
            // Run with `let+` under the handler where it is written.
            let g = do { let+ made = make(); made } hn { throw(m) => fn(n: i64) { 0 }, };
            <5 | g | println;
            // What flows into a callee is demanded by the callee.
            let c = do <make() | apply5_effectful hn { throw(m) => -3, };
            <c | println;
            // Stored, it carries its row on its type, and never runs unused.
            let t = (make(), 1);
            <t.1 | println;"#,
        ),
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["-1", "0", "-3", "1"]);
}

#[test]
fn a_delayed_computation_that_escapes_its_handler_is_refused() {
    let refused = [
        // Written under the handler, returned out of it, used outside.
        (
            "let g = do { let- f = make(); f } hn { throw(m) => fn(n: i64) { 0 }, };\n<5 | g | println;",
            "`main` performs `Exn`",
        ),
        // Handed to a callee that promises a pure arrow, even under a
        // handler: running it performs `Exn` inside `apply5`.
        (
            "let- f = make();\nlet b = do <f | apply5 hn { throw(m) => -2, };\n<b | println;",
            "`apply5` takes `f` with a pure arrow but `f` performs `Exn`",
        ),
        // Stored in a variant whose payload is declared pure.
        ("let h = Held::Holds(make());\n<0 | println;", "performs `Exn`"),
    ];
    for (index, (body, expected)) in refused.iter().enumerate() {
        let (stdout, stderr, ok) = run_sl_with(
            &[],
            &format!("slc_test_delayed_refused_{index}.sl"),
            &delayed_program(body),
        );
        assert!(!ok, "{body}: stdout {stdout}");
        assert!(stderr.contains(expected), "{body}: {stderr}");
    }
}

#[test]
fn a_declaration_returning_a_delayed_computation_preserves_its_row() {
    let source = format!(
        "{DELAYED_DECLS}
        func later() -> (i64 -> i64) {{ let- f = make(); f }}

        proc main | (exit: -i32) / {{IO}} {{ <0 | exit> }}"
    );
    let (_, stderr, ok) = run_sl_with(&[], "slc_test_delayed_returned.sl", &source);
    assert!(!ok);
    assert!(stderr.contains("`later` hands on a value that performs `Exn`"), "{stderr}");
}

// A row follows the value that carries it, wherever names cannot
// (`docs/design-notes/rows-in-types.md`).
const ROWS_DECLS: &str = r#"hook Exn { func throw(m: String) -> i64; }

func risky(x: i64) -> i64 / {Exn} { <"boom" | throw }
func inc(x: i64) -> i64 { <(x, 1) | add }
func app<E>(f: (i64 -> i64 / {..E}), x: i64) -> i64 / {..E} { <x | f }
func make() -> (i64 -> i64) / {Exn} {
    <"made" | throw;
    fn(n: i64) { <(n, 1) | add }
}
"#;

fn rows_program(body: &str) -> String {
    format!("{ROWS_DECLS}\nproc main | (exit: -i32) / {{IO}} {{\n{body}\n<0 | exit>\n}}\n")
}

#[test]
fn a_row_follows_its_value_where_names_cannot() {
    // Each performs `Exn` where no handler answers it. Today every one of
    // them is accepted and then fails with "no handler for operation".
    let refused = [
        // A lambda written under a handler and called after it.
        "let f = do { fn(x: i64) { <\"late\" | throw } } hn { throw(m) => fn(x: i64) { 0 }, };\n<1 | f | println;",
        // A higher-order global handed on as a value.
        "let+ h = app;\n<(risky, 1) | h | println;",
        // A function laundered through a `let`.
        "let+ f = risky;\n<1 | f | println;",
        // A row variable instantiated at a later stage of a chain.
        "<1 | inc | x => (risky, x) | app | println;",
    ];
    for (index, body) in refused.iter().enumerate() {
        let (stdout, stderr, ok) =
            run_sl_with(&[], &format!("slc_test_rows_refused_{index}.sl"), &rows_program(body));
        assert!(!ok, "{body}: stdout {stdout}");
        assert!(stderr.contains("`main` performs `Exn`"), "{body}: {stderr}");
    }
    // Each performs `Exn` under the handler around the point where it runs.
    // Today the first is refused where the lambda is written, and the last
    // is refused as a stored delayed computation.
    let accepted = [
        "let+ f = fn(x: i64) { <\"late\" | throw };\nlet r = do <1 | f hn { throw(m) => -1, };\n<r | println;",
        "let+ h = app;\nlet r = do <(risky, 1) | h hn { throw(m) => -1, };\n<r | println;",
        "let+ f = risky;\nlet r = do <1 | f hn { throw(m) => -1, };\n<r | println;",
        "let r = do <1 | inc | x => (risky, x) | app hn { throw(m) => -1, };\n<r | println;",
        "let t = (make(), 1);\nlet r = do <5 | t.0 hn { throw(m) => -1, };\n<r | println;",
    ];
    for (index, body) in accepted.iter().enumerate() {
        let (stdout, stderr, ok) =
            run_sl_with(&[], &format!("slc_test_rows_accepted_{index}.sl"), &rows_program(body));
        assert!(ok, "{body}: {stderr}");
        assert_eq!(stdout, "-1\n", "{body}");
    }
}

#[test]
fn a_delayed_computation_carries_a_row_variable_on_its_type() {
    let source = format!(
        "{ROWS_DECLS}
        func wrap<E>(k: ((,) -> (i64 -> i64) / {{..E}})) -> i64 / {{..E}} {{
            let- f = <(,) | k;
            <5 | f
        }}

        proc main | (exit: -i32) / {{IO}} {{
            let r = do <(fn(u: (,)) {{ make() }}) | wrap hn {{ throw(m) => -1, }};
            <r | println;
            <0 | exit>
        }}"
    );
    let (stdout, stderr, ok) = run_sl_with(&[], "slc_test_rows_delayed_variable.sl", &source);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "-1\n");
}

#[test]
fn a_run_has_no_step_cap_by_default() {
    let (stdout, stderr, ok) = run_sl_with(&[], "slc_test_long_loop.sl", LONG_LOOP);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "100000\n");
}

#[test]
fn a_tail_resuming_handler_runs_as_long_as_the_program_does() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_long_handled_loop.sl",
        r#"hook Tick { func tick() -> (,); }

        func spin(n: i64) -> i64 / {Tick} {
            of (<(n, 0) | eq) {
                True => 0,
                False => { tick(); <(n, 1) | sub | spin },
            }
        }

        proc main | (exit: -i32) / {IO} {
            let r = do (<100000 | spin) hn { tick(): resume => <(,) | resume, };
            <r | println;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "0\n");
}

#[test]
fn fuel_caps_the_machine_steps_of_a_run() {
    let (stdout, stderr, ok) = run_sl_with(&["--fuel", "1000"], "slc_test_fuel_cap.sl", LONG_LOOP);
    assert!(!ok, "stdout: {stdout}");
    assert!(stderr.contains("evaluation diverged (fuel exhausted)"), "stderr: {stderr}");
}

#[test]
fn fuel_without_a_number_reports_usage() {
    for args in [&["--fuel"][..], &["--fuel", "x"][..]] {
        let (_, stderr, ok) = run_sl_with(args, "slc_test_fuel_usage.sl", LONG_LOOP);
        assert!(!ok, "{args:?}");
        assert!(
            stderr.contains("usage: slc run [--fuel N] [--interpret] <file.sl>"),
            "{args:?}: {stderr}"
        );
    }
}

#[test]
fn builtin_add() {
    let dir = std::env::temp_dir().join("slc_test_add.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <add(1, 2) | println; <0 | exit> }")
        .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("3"));
}

#[test]
fn builtin_println() {
    let dir = std::env::temp_dir().join("slc_test_println.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <42 | println; <0 | exit> }").unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("42"));
}

#[test]
fn program_output_appears_in_order() {
    let dir = std::env::temp_dir().join("slc_test_println_vs_final_value.sl");
    std::fs::write(
        &dir,
        "proc main | (exit: -i32) / {IO} { <\"program output\" | println; <42 | println; <0 | exit> }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "program output\n42\n");
}

#[test]
fn a_program_prints_only_what_it_prints() {
    // The entry point is a command, so there is no final value to report:
    // output is exactly what the program printed.
    let dir = std::env::temp_dir().join("slc_test_no_final_value.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <42 | println; <0 | exit> }").unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn lambda_application() {
    let dir = std::env::temp_dir().join("slc_test_lambda.sl");
    std::fs::write(
        &dir,
        "proc main | (exit: -i32) / {IO} { <fn(x: +i32) -> i32 { x }(5) | println; <0 | exit> }",
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
        "proc main | (exit: -i32) / {IO} { <str_concat(int_to_str(1), int_to_str(2)) | println; <0 | exit> }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("12"));
}

#[test]
fn comparison() {
    let dir = std::env::temp_dir().join("slc_test_cmp.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <eq(1, 1) | println; <0 | exit> }")
        .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("true"));
}

#[test]
fn division_by_zero_rejected() {
    let dir = std::env::temp_dir().join("slc_test_div.sl");
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <div(1, 0) | println; <0 | exit> }")
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
        r#"proc main | (exit: -i32) / {IO} {
    let ok = fn(n: +i64) -> i32 { <n | println; <1 | exit> };
    let invalid = fn(s: +String) -> i32 { <s | println; <2 | println; <2 | exit> };
    let overflow = fn(s: +String) -> i32 { <s | println; <3 | println; <3 | exit> };
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
        r#"proc main | (exit: -i32) / {IO} {
    let ok = fn(n: +i64) -> i32 { <n | println; <1 | exit> };
    let invalid = fn(s: +String) -> i32 { <s | println; <2 | println; <2 | exit> };
    let overflow = fn(s: +String) -> i32 { <s | println; <3 | println; <3 | exit> };
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
        r#"proc main | (exit: -i32) / {IO} {
    let ok = fn(n: +i64) -> i32 { <n | println; <1 | exit> };
    let invalid = fn(s: +String) -> i32 { <s | println; <2 | println; <2 | exit> };
    let overflow = fn(s: +String) -> i32 { <s | println; <3 | println; <3 | exit> };
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
        "proc main | (exit: -i32) / {IO} { <of False { True => of (<(1, 0) | div | x => (x, 1) | eq) { True => 1, _ => 2 }, _ => 2 } | println; <0 | exit> }",
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
        "proc main | (exit: -i32) / {IO} { <of True { True => 3, _ => of (<(1, 0) | div | x => (x, 1) | eq) { True => 3, _ => 4 } } | println; <0 | exit> }",
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
    std::fs::write(
        &dir,
        "proc main | (exit: -i32) / {IO} { <(10, 3) | sub | x => (x, 2) | sub | println; <0 | exit> }",
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert!(stdout.contains("5"));
}

#[test]
fn out_of_range_index_rejected() {
    let dir = std::env::temp_dir().join("slc_test_oob.sl");
    std::fs::write(
        &dir,
        r#"proc main | (exit: -i32) / {IO} { <("ab", 5) | index | println; <0 | exit> }"#,
    )
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
        r#"proc main | (exit: -i32) / {IO} { <("abc", 1, 9) | substring | println; <0 | exit> }"#,
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
        r#"proc parse<E>(input: +String) | (ok: (-String / {..E}) & err: (-String / {..E})) / {..E} {
            of (<(input, "ok") | eq) { True => <"parsed" | ok>, _ => <"failed" | err> }
        }
        proc main | (exit: -i32) / {IO} {
            let ok = fn(value: +String) -> i32 { <("ok: ", value) | add | println; <0 | exit> };
            let err = fn(message: +String) -> i32 { <("err: ", message) | add | println; <1 | exit> };
            <"ok" | parse | (ok & err)>
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
        r#"proc parse<E>(input: +String) | (ok: (-String / {..E}) & err: (-String / {..E})) / {..E} {
            of (<(input, "ok") | eq) { True => <"parsed" | ok>, _ => <"failed" | err> }
        }
        proc main | (exit: -i32) / {IO} {
            let ok = fn(value: +String) -> i32 { <("ok: ", value) | add | println; <0 | exit> };
            let err = fn(message: +String) -> i32 { <("err: ", message) | add | println; <1 | exit> };
            <"bad" | parse | (ok & err)>
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
        r#"proc parse_json<E>(input: +String) | (ok: (-String / {..E}) & err: (-String / {..E})) / {..E} {
            let start = <(input, 0) | skip_ws;
            of (<(start, (<input | str_len)) | lt) {
                True => of (<(input, start) | index) {
                    '0'..='9' => <(input, start, (<(start, 1) | add)) | substring | ok>,
                    _ => <"expected JSON value" | err>
                },
                _ => <"empty input" | err>,
            }
        }
        proc main | (exit: -i32) / {IO} {
            let ok = fn(value: +String) -> i32 { <("parsed: ", value) | add | println; <0 | exit> };
            let err = fn(message: +String) -> i32 { <("error: ", message) | add | println; <1 | exit> };
            <"x" | parse_json | (ok & err)>
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
        func k(return: -i32) <- Color {{
            mu Color {{
                Red => <0 | return>,
                Green => <1 | return>,
                Blue => <2 | return>,
            }}
        }}
        proc main | (exit: -i32) / {{IO}} {{
            <mu i32 {{ out <= <Color::{variant} | k | out> }} | println;
            <0 | exit>
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

        func shout(name: +String, code: +i32) -> i32 / {IO} {
            <name | println;
            code
        }

        func dispatch(k: i32) <- Color / {IO} {
            mu Color {
                Red => <("red", 0) | shout | k>,
                Green => <("green", 1) | shout | k>,
                Blue => <("blue", 2) | shout | k>,
            }
        }

        proc main | (exit: -i32) / {IO} {
            <mu i32 { out <= <Color::Green | dispatch | out> } | println;
            <0 | exit>
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

        func boom(code: +i32) -> i32 / {IO} {
            <"BOOM" | println;
            code
        }

        func dispatch(k: i32) <- Color / {IO} {
            mu Color {
                Red => <3 | boom | k>,
                Green => <4 | boom | k>,
                Blue => <5 | boom | k>,
            }
        }

        proc main | (exit: -i32) / {IO} {
            <mu i32 { out <= {
                let consumer = <out | dispatch;
                7
            } } | println;
            <0 | exit>
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
        r#"proc main | (exit: -i32) / {IO} {
            let status = do {
                mu i32 { done <= <"does-not-exist.sl" | fs::read | (mu String {
                    source => { <("unexpectedly read ", source) | add | println; <1 | done> },
                } & mu String {
                    message => { <("failed: ", message) | add | println; <0 | done> },
                })> }
            } fs::real;
            <status | exit>
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
            r#"proc main | (exit: -i32) / {{IO}} {{
            let status = do {{
                mu i32 {{ finish <= {{
                    let failed = mu String {{ message => {{ <message | println; <1 | finish> }} }};
                    <({path:?}, "written") | fs::write | (mu unit {{
                        done => <{path:?} | fs::read | (mu String {{
                            source => {{ <source | println; <0 | finish> }},
                        }} & failed)>,
                    }} & failed)>
                }} }}
            }} fs::real;
            <status | exit>
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
        r#"func report(message: +String, exit: -i32) -> (;) / {IO} {
            <message | println;
            <1 | exit>
        }

        proc main | (exit: -i32) / {IO} {
            char_at("slant", 1, fn(second: +char) -> (;) {
                <second | println;
                char_at("slant", 9, fn(unexpected: +char) -> (;) {
                    (<("unexpectedly found something", exit) | report)
                }, fn(message: +String) -> (;) {
                    <message | println;
                    <0 | exit>
                })
            }, fn(message: +String) -> (;) { (<(message, exit) | report) })
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.lines().next() == Some("l"), "stdout: {stdout}");
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

        func show(out: -String) <- Reading {
            mu Reading {
                Reading { value, unit } => <((<value | int_to_str), unit) | add | out>,
            }
        }

        func total(out: -i64) <- (+i64, +i64) {
            mu (+i64, +i64) {
                (left, right) => <(left, right) | add | out>,
            }
        }

        proc main | (exit: -i32) / {IO} {
            <mu i64 { answer <= <(2, 40) | (<answer | total)> } | println;
            <mu String { answer <= <Reading { value: 42, unit: "m" } | show | answer> } | println;
            <0 | exit>
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

        func sum(d: D) -> i64 {
            of d {
                D { left: a, right: b } => (<(a, b) | add),
                _ => 0,
            }
        }

        proc main | (exit: -i32) / {IO} {
            let d = D { left: 10, right: 20 };
            <(<d | sum) | println;
            <0 | exit>
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
        r#"proc main | (exit: -i32) / {IO} {
            <mu i64 { k <= {
                <"before" | println;
                <1 | k>;
                <"after" | println
            } } | println;
            <0 | exit>
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

        func code(return: -i64) <- Color {
            mu Color {
                Red => <0 | return>,
                Green => <1 | return>,
                Blue => <2 | return>,
            }
        }

        proc main | (exit: -i32) / {IO} {
            <mu i64 { answer <= <Color::Blue | code | answer> } | println;
            <0 | exit>
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
        "proc bad(x: +i32) | (k: -i32) { k(x) }\nproc main | (exit: -i32) / {IO} { <0 | exit> }",
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
        r#"func f(k: -i32) <- i32 {
            <(mu i32 { outer <= <42 | outer> }) | k>
        }
        proc main | (exit: -i32) / {IO} { <0 | exit> }"#,
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
        "/../../examples/programs/json_parser.sl"
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
        "/../../examples/programs/json_parser.sl"
    ))
    .unwrap();
    let path = std::env::temp_dir().join("slc_test_json_unchanged.sl");
    std::fs::write(&path, source).unwrap();
    let (stdout, stderr, ok) = run_sl(path.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        r#"parsed: {"name":"slant","tags":[1,2,-3.25],"active":true,"none":null,"escaped":"a\"b\u0041"}"#,
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
        "/../../examples/programs/json_parser.sl"
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
    std::fs::write(&dir, "proc main | (exit: -i32) / {IO} { <42 | fs::close; <0 | exit> }")
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
            r#"proc main | (exit: -i32) / {{IO}} {{
                let status = do {{
                    mu i32 {{ done <= {{
                        let fail = mu String {{ m => {{ <m | println; <1 | done> }} }};
                        let fh = mu {{ k <= <"{}" | fs::open | (k & fail)> }};
                        <fh | fs::close;
                        let line = mu {{ k <=
                            <fh | fs::read_line | (k & mu unit {{ e => {{ <"eof" | println; <1 | done> }} }})>
                        }};
                        <line | println;
                        <0 | done>
                    }} }}
                }} fs::real;
                <status | exit>
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
fn a_program_mocks_the_file_system_with_a_handler_of_its_own() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_fs_mock.sl",
        r#"cite fs::read_file;

        // Every file holds its own name, and nothing touches the disk.
        func canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {..E} {
            do <(,) | program hn {
                read_file(path): resume => <::0(<("canned ", path) | add) | resume,
                fs::write_file(path, text): resume => <::0((,)) | resume,
                fs::open_file(path): resume => <::1("not supported") | resume,
                fs::read_line_of(file): resume => <::1((,)) | resume,
                fs::close_file(file): resume => <(,) | resume,
                fs::file_exists(path): resume => <False | resume,
            }
        }

        proc main | (exit: -i32) / {IO} {
            let text = <(fn(u: (,)) {
                mu String { k <= <"nowhere.txt" | fs::read | (k & mu String { m => <"failed" | k> })> }
            }) | canned;
            <text | println;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "canned nowhere.txt\n");
}

#[test]
fn a_computation_is_handed_to_a_handler_as_fn_braces() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_fn_braces.sl",
        r#"func canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {..E} {
            do <(,) | program hn {
                fs::read_file(path): resume => <::0(<("canned ", path) | add) | resume,
                fs::write_file(path, text): resume => <::0((,)) | resume,
                fs::open_file(path): resume => <::1("not supported") | resume,
                fs::read_line_of(file): resume => <::1((,)) | resume,
                fs::close_file(file): resume => <(,) | resume,
                fs::file_exists(path): resume => <False | resume,
            }
        }

        func shout(path: String) -> String / {fs::Fs} {
            mu String { k <= <path | fs::read | (mu String { t => <(t, "!") | add | k> } & mu String { m => <"failed" | k> })> }
        }

        proc main | (exit: -i32) / {IO} {
            // `fn { … }` is `fn(u: (,)) { … }`.
            let braces = <(fn { <"a.txt" | shout }) | canned;
            let unit = <(fn(u: (,)) { <"a.txt" | shout }) | canned;
            <braces | println;
            <unit | println;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "canned a.txt!\ncanned a.txt!\n");
}

#[test]
fn a_program_ending_in_a_cut_is_handed_to_a_command_handler() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_command_handler.sl",
        r#"// The program leaves through `exit`, so it is `(;)`: a command's exit.
        proc canned<E> | (program: ((;) / {fs::Fs, ..E})) / {..E} {
            do program hn {
                fs::read_file(path): resume => <::0(<("canned ", path) | add) | resume,
                fs::write_file(path, text): resume => <::0((,)) | resume,
                fs::open_file(path): resume => <::1("not supported") | resume,
                fs::read_line_of(file): resume => <::1((,)) | resume,
                fs::close_file(file): resume => <(,) | resume,
                fs::file_exists(path): resume => <False | resume,
            }
        }

        proc main | (exit: -i32) / {IO} {
            <(,) | canned | (fn {
                let text = mu { k <= <"x.txt" | fs::read | (k & mu String { m => <1 | exit> })> };
                <text | println;
                <0 | exit>
            })>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "canned x.txt\n");
}

#[test]
fn a_program_ending_in_a_cut_runs_under_real_command() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_real_command.sl",
        r#"proc main | (exit: -i32) / {IO} {
            do {
                <"surely/not/here.txt" | fs::read | (
                    mu String { text => <1 | exit> }
                    & mu String { why => { <"cannot read" | println; <0 | exit> } }
                )>
            } fs::real
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "cannot read\n");
}

#[test]
fn a_handler_clause_names_its_operation_by_path() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_fs_clause_path.sl",
        r#"func canned<+A, E>(program: ((,) -> A / {fs::Fs, ..E})) -> A / {..E} {
            do <(,) | program hn {
                fs::read_file(path): resume => <::0(<("canned ", path) | add) | resume,
                fs::write_file(path, text): resume => <::0((,)) | resume,
                fs::open_file(path): resume => <::1("not supported") | resume,
                fs::read_line_of(file): resume => <::1((,)) | resume,
                fs::close_file(file): resume => <(,) | resume,
                fs::file_exists(path): resume => <False | resume,
            }
        }

        proc main | (exit: -i32) / {IO} {
            let text = <(fn(u: (,)) {
                mu String { k <= <"nowhere.txt" | fs::read | (k & mu String { m => <"failed" | k> })> }
            }) | canned;
            <text | println;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "canned nowhere.txt\n");
}

#[test]
fn a_partial_file_handler_forwards_writes_to_real_command() {
    let path = std::env::temp_dir().join("slc_test_forwarded_write.txt");
    let _ = std::fs::remove_file(&path);
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_fs_forwarding.sl",
        &format!(
            r#"proc main | (exit: -i32) / {{IO}} {{
                do {{
                    do {{
                        let outcome = <("{}", "forwarded") | fs::write_file;
                        let contents = mu String {{ done <=
                            <"ignored" | fs::read | (done & mu String {{ message => <1 | exit> }})>
                        }};
                        <contents | println;
                        <0 | exit>
                    }} hn {{
                        fs::read_file(path): resume => <::0("intercepted") | resume,
                        _ => forward
                    }}
                }} fs::real
            }}"#,
            path.display(),
        ),
    );
    let written = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "intercepted\n");
    assert_eq!(written.unwrap(), "forwarded");
}

#[test]
fn a_file_operation_needs_a_handler_around_it() {
    let (_, stderr, ok) = run_sl_with(
        &[],
        "slc_test_fs_unhandled.sl",
        r#"proc main | (exit: -i32) / {IO} {
            <"x.txt" | fs::read | (mu String { t => <0 | exit> } & mu String { m => <1 | exit> })>
        }"#,
    );
    assert!(!ok);
    assert!(stderr.contains("`main` performs `fs::Fs`"), "{stderr}");
}

#[test]
fn the_prelude_is_available_and_shadowable() {
    // Prelude declarations are in scope without an import: `Display` and
    // `to_string`, and `IO`.
    let dir = std::env::temp_dir().join("slc_test_prelude.sl");
    std::fs::write(
        &dir,
        r#"func double(n: +i64) -> i64 { (<(n, 2) | mul) }
        proc main | (exit: -i32) / {IO} {
            <(<21 | double | to_string) | println;
            <(<True | fmt) | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["42", "true"]);

    // A program's own definition shadows the prelude's.
    let dir = std::env::temp_dir().join("slc_test_prelude_shadow.sl");
    std::fs::write(
        &dir,
        r#"func to_string(n: +i64) -> String { "mine" }
        proc main | (exit: -i32) / {IO} { <(<7 | to_string) | println; <0 | exit> }"#,
    )
    .unwrap();
    let (stdout, _, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok);
    assert_eq!(stdout.trim(), "mine");

    // The program's text precedes the prelude, so a diagnostic keeps the
    // program's own line and column.
    let dir = std::env::temp_dir().join("slc_test_prelude_spans.sl");
    std::fs::write(
        &dir,
        r#"proc main | (exit: -i32) / {IO} {
    <(1, "x") | add | println;
    <0 | exit>
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
        r#"func plus(a: i64, b: i64) -> i64 { (<(a, b) | add) }
        func apply2(f: ((+i64, +i64) -> +i64), x: i64, y: i64) -> i64 { f(x, y) }
        proc main | (exit: -i32) / {IO} {
            <(<(1, 2) | plus) | println;
            let g = plus;
            <g(10, 20) | println;
            <(<(plus, 3, 4) | apply2) | println;
            <0 | exit>
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
    // and `<` says it is the value flowing in instead.
    let dir = std::env::temp_dir().join("slc_test_flow_head.sl");
    std::fs::write(
        &dir,
        r#"func double(n: i64) -> i64 { (<(n, 2) | mul) }
        func describe(out: -String) <- ((+i64 -> +i64)) {
            fn(f: (+i64 -> +i64)) { <"a function" | out> }
        }
        proc main | (exit: -i32) / {IO} {
            <21 | double | println;             // a value heads it: apply
            let quadruple = double | double;   // a function heads it: compose
            <5 | quadruple | println;
            <mu String { s <= <double | describe | s> } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["42", "20", "a function"]);
}

#[test]
fn a_row_closes_the_chain_and_travels_whole() {
    // A command's exits are the chain's closing stage, written as one
    // bundle — and a row is first class: a command can take one and hand
    // it on unopened.
    let dir = std::env::temp_dir().join("slc_test_row_forms.sl");
    std::fs::write(
        &dir,
        r#"proc classify(n: i64) | (found: i64 & missing: String) {
            of (<(n, 0) | gt) { True => <n | found>, _ => <"negative" | missing> }
        }
        proc forward(n: i64) | (row: (-i64 & -String)) { <n | classify | row> }
        proc main | (exit: -i32) / {IO} {
            <mu i64 { ok <= <7 | classify | (ok & mu +String { s => <s | str_len | ok> })> } | println;
            <mu i64 { ok <= <(0, 1) | sub | forward | (ok & mu +String { s => <s | str_len | ok> })> } | println;
            <0 | exit>
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
        r#"hook Tag { func tag(label: +String, n: +i64) -> i64; }
        proc main | (exit: -i32) / {IO} {
            <do (<("ten", 7) | tag) hn { tag(l, n): k => (<(n, 10) | mul | k), return(m) => m } | println;
            <do (<("len", 7) | tag) hn { tag(l, n) => (<((<l | str_len), n) | add), return(m) => m } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["70", "10"]);
}

#[test]
fn the_four_logical_units_are_nullary_connectives() {
    // `(,)` and `(&)` have one value each; `(|)` has none, so a function from
    // it never produces one and its consumer has no arms; `(;)` is what a
    // command is.
    let dir = std::env::temp_dir().join("slc_test_nullary_units.sl");
    std::fs::write(
        &dir,
        r#"func unit_value() -> (,) { (,) }
        func top_value() -> (&) { (&) }
        func use_empty<+T>(empty: (|)) -> T { of empty {} }
        func absurd(out: -i64) <- (|) { mu (|) {} }
        proc halt | (exit: -i32) -> (;) { <0 | exit> }
        proc main | (exit: -i32) / {IO} {
            of unit_value() { (,) => <"unit" | println };
            top_value();
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["unit"]);
}

#[test]
fn a_stdlib_module_is_reached_by_path_or_use_and_not_otherwise() {
    let dir = std::env::temp_dir().join("slc_test_stdlib_reach.sl");
    std::fs::write(
        &dir,
        r#"cite num::max;
        proc main | (exit: -i32) / {IO} {
            <(<(3, 7) | num::min) | println;
            <(<(3, 7) | max) | println;
            <0 | exit>
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
        r#"proc main | (exit: -i32) / {IO} { <(<(3, 7) | min) | println; <0 | exit> }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("min"), "stderr: {stderr}");

    // A program's own module shadows a stdlib module of the same name, whole.
    let dir = std::env::temp_dir().join("slc_test_stdlib_shadow_mod.sl");
    std::fs::write(
        &dir,
        r#"sect num { pub func min(a: +i64, b: +i64) -> i64 { (<(a, 100) | add) } }
        proc main | (exit: -i32) / {IO} { <(<(3, 7) | num::min) | println; <0 | exit> }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "103");

    // A library-side diagnostic names its unit.
    let dir = std::env::temp_dir().join("slc_test_stdlib_diag.sl");
    std::fs::write(
        &dir,
        r#"cite list::List::*;
        proc main | (exit: -i32) / {IO} { <(Nil, "x") | list::nth | (exit & exit)> }"#,
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
        r#"proc main | (exit: i32) / {IO} {
            <mu i64 { out <= <("answer", 42) | trace::tap | out> } | println;
            // A row slot wants a consumer, and `select` is what builds one.
            <mu i64 { out <=
                <"nope" | parse_int | (out
                    & mu String { m => <7 | out> }
                    & mu String { m => <9 | out> })>
            } | println;
            <mu i64 { out <=
                <"35" | parse_int | (out
                    & mu String { m => <7 | out> }
                    & mu String { m => <9 | out> })>
            } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.split_whitespace().collect::<Vec<_>>(), ["answer", "42", "42", "7", "35"]);
}

#[test]
fn display_formats_through_bounded_impls() {
    let dir = std::env::temp_dir().join("slc_test_display.sl");
    std::fs::write(
        &dir,
        r#"cite list::List;
        proc main | (exit: -i32) / {IO} {
            <fmt(42) | println;
            <fmt("plain") | println;
            <fmt(False) | println;
            <(<7 | to_string) | println;
            let xs = List::Cons(1, List::Cons(2, List::Nil));
            <fmt(xs) | println;
            <(<xs | to_string) | println;
            <fmt(List::Cons(xs, List::Cons(List::Nil, List::Nil))) | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        ["42", "plain", "false", "7", "[1, 2]", "[1, 2]", "[[1, 2], []]"],
        "stdout: {stdout}"
    );

    // A type without an impl is rejected, not garbled.
    let dir = std::env::temp_dir().join("slc_test_display_missing.sl");
    std::fs::write(
        &dir,
        r#"data P { x: i64 }
        proc main | (exit: -i32) / {IO} { <fmt(P { x: 1 }) | println; <0 | exit> }"#,
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
        r#"cite list::List;
        cite list::List::*;
        enum Mine { Nil, Cons(i64, Mine) }
        func total(xs: List<i64>) -> i64 {
            of xs { Nil => 0, Cons(n, rest) => (<(n, (<rest | total)) | add) }
        }
        proc main | (exit: -i32) / {IO} { <Cons(40, Cons(2, Nil)) | total | println; <0 | exit> }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");

    // Without an import, an ambiguous bare pattern is an error — never a
    // silent catch-all binder. The program reaches `list`, so its `List` is
    // loaded and competes with `Mine` for `Nil` and `Cons`; a program that
    // reaches no `list` has no such competition, since the module is not
    // loaded at all.
    let dir = std::env::temp_dir().join("slc_test_use_ambiguous.sl");
    std::fs::write(
        &dir,
        r#"cite list::List;
        enum Mine { Nil, Cons(i64, Mine) }
        func count(xs: Mine) -> i64 {
            of xs { Nil => 0, Cons(_, rest) => (<(1, count(rest)) | add) }
        }
        proc main | (exit: -i32) / {IO} { <count(Mine::Nil) | println; <0 | exit> }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("variant of more than one enum"), "stderr: {stderr}");

    // Colliding imports are an error at the `use`.
    let dir = std::env::temp_dir().join("slc_test_use_collision.sl");
    std::fs::write(
        &dir,
        r#"cite list::List::*;
        enum Mine { Nil }
        cite Mine::*;
        proc main | (exit: -i32) / {IO} { 0 | exit> }"#,
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
        menu Stream2<+T> { head: T, tail: Stream2<T> }

        spec Describe { func describe(self: +Self) -> String; }

        impl Describe for Config {
            func describe(self: +Config) -> String {
                (<(self.name, " with ") | add | x => (x, fmt(self.retries)) | add | x => (x, " retries") | add)
            }
        }
        impl Describe for Report {
            func describe(self: +Report) -> String { "a report sink" }
        }
        impl<+T: Display> Describe for Stream2<T> {
            func describe(self: +Stream2<T>) -> String {
                (<("stream starting ", fmt(self.head)) | add)
            }
        }

        func config() -> Config {
            mu Config { retries <= <3 | retries>, name <= <"slant" | name> }
        }
        func printer(out: -i32) -> Report {
            mu Report { Report { value, label } => <value | out> }
        }
        func ones() -> Stream2<i64> {
            mu Stream2 { head: out <= <1 | out>, tail: out <= <ones() | out> }
        }
        func label<-T: Describe>(x: T) -> String { describe(x) }

        proc main | (exit: -i32) / {IO} {
            <describe(config()) | println;
            <describe((<exit | printer)) | println;
            <describe(ones()) | println;
            <config() | label | println;
            <(<exit | printer) | label | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "slant with 3 retries",
            "a report sink",
            "stream starting 1",
            "slant with 3 retries",
            "a report sink",
        ],
        "stdout: {stdout}"
    );
}

#[test]
fn json_parser_rejects_trailing_characters() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/programs/json_parser.sl"
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
        "/../../examples/programs/json_parser.sl"
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
        "proc main | (exit: -i32) / {IO} {
             let show = mu +i64 { n => { <(n, 2) | mul | println; <0 | exit> } };
             <21 | show>;
             <0 | exit>
         }",
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "42");
}

#[test]
fn a_value_meets_a_slot_at_the_mirrored_spelling_of_its_type() {
    // `(A ; B)` and `(B ; A)` are different types. A function stored where the
    // other orientation is declared is refused.
    let dir = std::env::temp_dir().join("slc_test_par_commutes.sl");
    std::fs::write(
        &dir,
        r#"menu Deliver { deliver: (i64 -> String) }
        func deliver_i64(out: String) <- i64 {
            mu i64 { n => <("the number ", (<n | int_to_str)) | add | out> }
        }
        func delivers() -> Deliver { mu Deliver { deliver <= <deliver_i64 | deliver> } }

        menu Render { render: (-String -> -i64) }
        func render_i64(n: i64) -> String { (<("n=", (<n | int_to_str)) | add) }
        func renders() -> Render { mu Render { render <= <render_i64 | render> } }

        proc main | (exit: i32) / {IO} {
            <42 | (<(,) | delivers).deliver | println;
            <mu String { s <= <7 | (<s | (<(,) | renders).render)> } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "the other orientation was accepted");
    assert!(stderr.contains("type:"), "{stderr}");
}

#[test]
fn a_value_is_stored_passed_and_returned_at_the_mirrored_spelling() {
    // The same refusal, wherever a value meets a declared type: a record
    // field, a `let` annotation, a variant's payload, an argument, and a
    // return.
    let dir = std::env::temp_dir().join("slc_test_par_stores.sl");
    std::fs::write(
        &dir,
        r#"func deliver_i64(out: String) <- i64 {
            mu i64 { n => <("the number ", (<n | int_to_str)) | add | out> }
        }
        data Holder { f: (i64 -> String) }
        enum Box1 { B((i64 -> String)) }
        func use_it(g: (i64 -> String)) -> String { <42 | g }
        func get() -> (i64 -> String) { deliver_i64 }

        proc main | (exit: i32) / {IO} {
            let h = Holder { f: deliver_i64 };
            <1 | h.f | println;
            let g: (i64 -> String) = deliver_i64;
            <2 | g | println;
            <deliver_i64 | use_it | println;
            <3 | (<(,) | get) | println;
            of Box1::B(deliver_i64) { B(k) => <4 | k | println };
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "the other orientation was accepted");
    assert!(stderr.contains("type:"), "{stderr}");
}

#[test]
fn a_value_is_turned_inside_a_written_structure_and_between_stages() {
    // A tuple, an alternative, and a stage's result, each meeting the other
    // spelling of its type, are refused.
    let (_, stderr, ok) = run_sl_with(
        &[],
        "slc_test_par_structures.sl",
        r#"func deliver_i64(out: String) <- i64 {
            mu i64 { n => <("the number ", (<n | int_to_str)) | add | out> }
        }
        func use_it(g: (i64 -> String)) -> String { <42 | g }
        func get_negative() -> (-String -> -i64) { deliver_i64 }

        proc main | (exit: i32) / {IO} {
            let t: ((i64 -> String), i64) = (deliver_i64, 1);
            <t.1 | t.0 | println;
            let s: ((i64 -> String) | i64) = ::0(deliver_i64);
            of s { ::0(f) => <5 | f | println, ::1(n) => <n | println> };
            <(,) | get_negative | use_it | println;
            <0 | exit>
        }"#,
    );
    assert!(!ok, "the other orientation was accepted");
    assert!(stderr.contains("type:"), "{stderr}");
}

#[test]
fn adapters_do_not_override_a_type_parameters_polarity() {
    let (_, stderr, ok) = run_sl_with(
        &[],
        "slc_test_par_constructor.sl",
        r#"cite list::List;
        cite list::List::*;

        func deliver_i64(out: String) <- i64 {
            mu i64 { n => <("the number ", (<n | int_to_str)) | add | out> }
        }

        proc main | (exit: i32) / {IO} {
            let xs: List<(i64 -> String)> = Cons(deliver_i64, Nil);
            <0 | exit>
        }"#,
    );
    assert!(!ok);
    assert!(stderr.contains("negative type"), "{stderr}");
}

#[test]
fn a_bounded_function_and_a_negative_method_read_either_way_in_a_chain() {
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_bounded_stages.sl",
        r#"func emit<+T: Display>(out: String) <- T {
            fn(x: T) { <x | fmt | out> }
        }

        spec Deliver { func deliver(out: String) <- Self; }

        impl Deliver for i64 {
            func deliver(out: String) <- i64 { fn(n: i64) { <("the number ", (<n | fmt)) | add | out> } }
        }

        proc main | (exit: -i32) / {IO} {
            <42 | emit | println;
            <mu String { s <= <7 | (<s | deliver)> } | println;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["42", "the number 7"]);
}

#[test]
fn an_alternative_is_resolved_against_the_sum_its_context_gives() {
    // `::1(v)` is the second alternative of whatever sum it meets: of two it
    // is the last, of three the middle. A return type, a `let` annotation and
    // a parameter each give the sum, and the payload takes that alternative's
    // type.
    let dir = std::env::temp_dir().join("slc_test_resolved_alternatives.sl");
    std::fs::write(
        &dir,
        r#"func pick(x: (i64 | Bool | String)) -> String {
            of x {
                ::0(n) => <n | int_to_str,
                ::1(b) => of b { True => "yes", _ => "no" },
                _ => "other",
            }
        }
        func middle() -> (i64 | Bool | String) { ::1(True) }
        proc main | (exit: i32) -> (;) / {IO} {
            let last: (i64 | String) = ::1("two");
            let third: (i64 | Bool | String) = ::2("three");
            <middle() | pick | println;
            <third | pick | println;
            <::0(5) | pick | println;
            let shown = of last { ::0(n) => <n | int_to_str, ::1(s) => s };
            <shown | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["yes", "other", "5", "two"]);
}

#[test]
fn an_alternative_outside_its_sum_is_refused() {
    let dir = std::env::temp_dir().join("slc_test_refused_alternatives.sl");
    std::fs::write(
        &dir,
        r#"func missing(out: String) <- (i64 | Bool | String) {
            mu (i64 | Bool | String) { ::0(n) => <"x" | out>, ::1(b) => <"y" | out> }
        }
        func twice(out: String) <- (i64 | String) {
            mu (i64 | String) { ::0(n) => <"x" | out>, ::1(s) => <"y" | out>, ::0(m) => <"z" | out> }
        }
        func beyond() -> (i64 | String) { ::2(1) }
        func wrong() -> (i64 | String) { ::1(1) }
        proc main | (exit: i32) { <0 | exit> }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "every alternative here is out of its sum");
    for expected in [
        "missing `::2`",
        "answers `::0` in more than one arm",
        "`::2` is out of range",
        "carries +String; this value has type +i64",
    ] {
        assert!(stderr.contains(expected), "missing {expected:?} in: {stderr}");
    }
}

#[test]
fn nesting_is_significant_and_a_position_needs_no_sum() {
    // `(A, (B, C))` has two components and `(A, B, C)` three, and so for sums.
    // An alternative is built by its position alone, so one whose sum nothing
    // names is no error — and one inside another is still checked against the
    // sum the outer one reveals.
    let dir = std::env::temp_dir().join("slc_test_nesting_is_significant.sl");
    std::fs::write(
        &dir,
        r#"func second(t: (i64, (i64, i64))) -> (i64, i64) { t.1 }
        func third(t: (i64, i64, i64)) -> i64 { t.2 }
        func nested(x: (i64 | (Bool | String))) -> String {
            of x {
                ::0(n) => <n | int_to_str,
                ::1(rest) => of rest { ::0(b) => of b { True => "yes", _ => "no" }, ::1(s) => s },
            }
        }
        func flat(x: (i64 | Bool | String)) -> String {
            of x { ::0(n) => <n | int_to_str, ::1(b) => "bool", ::2(s) => s }
        }
        proc main | (exit: i32) / {IO} {
            let unused = ::1(1);
            <(1, (2, 3)) | second | println;
            <(1, 2, 3) | third | println;
            <::1(::1("deep")) | nested | println;
            <::2("flat") | flat | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["(2, 3)", "3", "deep", "flat"]);

    // The inner alternative's payload is checked once the outer one says
    // which sum it is in.
    let dir = std::env::temp_dir().join("slc_test_nested_alternative_checked.sl");
    std::fs::write(
        &dir,
        r#"func nested(x: (i64 | (Bool | String))) -> i64 { 0 }
        proc main | (exit: i32) / {IO} { <::1(::1(5)) | nested | println; <0 | exit> }"#,
    )
    .unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "a nested alternative carries the wrong payload");
    assert!(stderr.contains("carries +String; this value has type +i64"), "stderr: {stderr}");
}

#[test]
fn a_joint_transfers_to_its_first_nonreturning_consumer() {
    let dir = std::env::temp_dir().join("slc_test_form_value.sl");
    std::fs::write(
        &dir,
        r#"proc main | (exit: i32) / {IO} {
            let answer = mu i64 { done <= {
                let first = mu i64 { n => { <n | println; <n | done> } };
                let second = mu String { s => { <s | println; <0 | done> } };
                let typed: ((-i64 ; -String) / {IO}) = (first ; second);
                <(7, "never") | typed>
            } };
            <answer | println;
            let sink = fn(n: i64) { <n | println };
            <8 | sink;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["7", "7", "8"]);
}

#[test]
fn binder_stages_keep_a_chain_flat() {
    // `s => e` names what flows in, so a stage's other arguments are written
    // where it stands instead of nesting the chain so far.
    let dir = std::env::temp_dir().join("slc_test_value_binder.sl");
    std::fs::write(
        &dir,
        r#"func odd(n: i64) -> Bool { (<(n, 2) | rem | x => (x, 1) | eq) }
        proc main | (exit: i32) / {IO} {
            <1 | stream::count_from | seq::of_stream
               | s => (odd, s) | seq::filter
               | s => (s, 4) | seq::take
               | seq::to_list | fmt | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("[1, 3, 5, 7]"), "stdout: {stdout}");
}

#[test]
fn a_consumer_binder_builds_a_row_from_the_rest_of_the_chain() {
    // `ok <= (ok & odd)` names the consumer the rest of the chain builds, so
    // each step that can fail supplies its failure exit and the chain carries on.
    let dir = std::env::temp_dir().join("slc_test_consumer_binder.sl");
    let program = |start: i64| {
        format!(
            r#"proc halve<E>(n: i64) | (ok: (-i64 / {{..E}}) & odd: (-String / {{..E}})) / {{..E}} {{
                of (<(n, 2) | rem | x => (x, 0) | eq) {{ True => <(n, 2) | div | ok>, _ => <"odd" | odd> }}
            }}
            proc main | (exit: i32) / {{IO}} {{
                let odd = mu String {{ m => {{ <m | println; <1 | exit> }} }};
                let quarter = mu i64 {{ out <=
                    <{start} | halve | ok <= (ok & odd) | halve | ok <= (ok & odd) | out>
                }};
                <quarter | println;
                <0 | exit>
            }}"#
        )
    };
    std::fs::write(&dir, program(12)).unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains('3'), "stdout: {stdout}");

    std::fs::write(&dir, program(6)).unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "6 halves to 3, which is odd: stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("odd"), "stdout: {stdout}");
}

#[test]
fn a_value_alone_is_not_opened_with_a_bracket() {
    // This used to pass every check and then crash in lowering.
    let dir = std::env::temp_dir().join("slc_test_stageless_open.sl");
    std::fs::write(&dir, "proc main | (exit: i32) { let x = <1; <0 | exit> }").unwrap();
    let (_, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok);
    assert!(stderr.contains("has none"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

#[test]
fn anonymous_data_types_display() {
    // The unit, tuples and choices carry `Display` up to eight components,
    // each rendered as it is written.
    let dir = std::env::temp_dir().join("slc_test_anonymous_display.sl");
    std::fs::write(
        &dir,
        r#"proc main | (exit: i32) / {IO} {
            <(<(1, "a") | fmt) | println;
            <(<(1, 2, 3, 4, 5, 6, 7, 8) | fmt) | println;
            let c: (i64 | String) = ::1("right");
            <(<c | fmt) | println;
            <(<((1, True), (,)) | fmt) | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    for expected in ["(1, a)", "(1, 2, 3, 4, 5, 6, 7, 8)", "::1(right)", "((1, true), (,))"] {
        assert!(stdout.contains(expected), "missing {expected}: {stdout}");
    }
}

#[test]
fn a_delayed_let_runs_at_each_demand_and_a_now_let_once() {
    // `let-` holds the block and runs it wherever the function is applied,
    // so its effect happens at each use; `let+` runs it where it is written.
    let dir = std::env::temp_dir().join("slc_test_delayed_let.sl");
    std::fs::write(
        &dir,
        r#"proc main | (exit: -i32) / {IO} {
            let- shout = { <"made" | println; fn(s: String) { <s | println } };
            <"a" | shout;
            <"b" | shout;
            let+ once = { <"once" | println; fn(s: String) { <s | println } };
            <"c" | once;
            <"d" | once;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["made", "a", "made", "b", "once", "c", "d"]);
}

#[test]
fn a_plain_let_follows_the_polarity_of_its_type() {
    // A function-producing block is negative, so a plain `let` delays it and
    // it runs at each use; a block producing a number runs where it is written.
    let dir = std::env::temp_dir().join("slc_test_plain_let_polarity.sl");
    std::fs::write(
        &dir,
        r#"proc main | (exit: -i32) / {IO} {
            let shout = { <"made" | println; fn(s: String) { <s | println } };
            <"a" | shout;
            <"b" | shout;
            let n = { <"computed" | println; 21 };
            <(n, n) | add | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["made", "a", "made", "b", "computed", "42"]);
}

#[test]
fn a_tuple_component_of_negative_type_runs_at_each_use() {
    // Delayed, the component runs each time it is used, and performs there.
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_by_name_component_each_use.sl",
        r#"proc main | (exit: -i32) / {IO} {
            let pair = (1, { <"made" | println; fn(s: String) { <s | println } });
            <"a" | pair.1;
            <"b" | pair.1;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["made", "a", "made", "b"]);

    // Computed first with `let+`, it runs once, here.
    let (stdout, stderr, ok) = run_sl_with(
        &[],
        "slc_test_by_name_component.sl",
        r#"proc main | (exit: -i32) / {IO} {
            let+ shout = { <"made" | println; fn(s: String) { <s | println } };
            let pair = (1, shout);
            <"a" | pair.1;
            <"b" | pair.1;
            <0 | exit>
        }"#,
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["made", "a", "b"]);
}

#[test]
fn a_bundle_item_that_ends_in_a_cut_runs_only_when_chosen() {
    let dir = std::env::temp_dir().join("slc_test_by_name_bundle.sl");
    std::fs::write(
        &dir,
        r#"proc pick<E>(c: Bool) | (then: (-> (;) / {..E}) & otherwise: (-> (;) / {..E})) / {..E} {
            of c { True => <(,) | then>, _ => <(,) | otherwise> }
        }

        proc main | (exit: -i32) / {IO} {
            <True | pick | ({ <"then" | println; <0 | exit> } & { <"otherwise" | println; <1 | exit> })>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["then"]);
}

#[test]
fn an_exit_named_as_a_command_runs_and_passed_on_it_does_not() {
    // `pick` names the exit it takes, which runs the delayed item; `forward`
    // passes both exits on unrun, so `pick` still runs only the one chosen.
    let dir = std::env::temp_dir().join("slc_test_exit_as_command.sl");
    std::fs::write(
        &dir,
        r#"proc pick<E>(c: Bool) | (then: (-> (;) / {..E}) & otherwise: (-> (;) / {..E})) / {..E} {
            of c { True => then, _ => otherwise }
        }

        proc forward<E>(c: Bool) | (then: (-> (;) / {..E}) & otherwise: (-> (;) / {..E})) / {..E} {
            <c | pick | (then & otherwise)>
        }

        proc main | (exit: -i32) / {IO} {
            <False | forward | ({ <"then" | println; <1 | exit> } & { <"otherwise" | println; <0 | exit> })>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["otherwise"]);
}

#[test]
fn what_flows_into_a_function_is_by_name() {
    // The block produces a function, so it is delayed as it flows into
    // `twice`, and runs at each of the two uses there.
    let dir = std::env::temp_dir().join("slc_test_by_name_flow_head.sl");
    std::fs::write(
        &dir,
        r#"func twice(g: (-> (String -> (,) / {IO}) / {IO})) -> (,) / {IO} {
            <"a" | g;
            <"b" | g
        }

        proc main | (exit: -i32) / {IO} {
            <{ <"made" | println; fn(s: String) { <s | println } } | twice;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["made", "a", "made", "b"]);
}

#[test]
fn a_trait_method_of_two_parameters_dispatches_on_its_self_component() {
    let dir = std::env::temp_dir().join("slc_test_binary_method.sl");
    std::fs::write(
        &dir,
        r#"spec Combine { func combine(self: Self, other: Self) -> Self; }
        impl Combine for i64 { func combine(self: i64, other: i64) -> i64 { (<(self, other) | add) } }
        impl Combine for String { func combine(self: String, other: String) -> String { (<(self, other) | add) } }

        proc main | (exit: -i32) / {IO} {
            <(<(1, 2) | combine) | println;
            <(<("a", "b") | combine) | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["3", "ab"]);
}

#[test]
fn a_match_on_true_and_false_is_exhaustive_without_a_wildcard() {
    // `bool` is the prelude's `enum Bool`, so its two variants cover it the
    // way any enum's variants do — on a literal and on a builtin's answer.
    let dir = std::env::temp_dir().join("slc_test_bool_exhaustive.sl");
    std::fs::write(
        &dir,
        r#"func describe(b: Bool) -> String {
            of b { True => "yes", False => "no" }
        }

        proc main | (exit: -i32) / {IO} {
            <True | describe | println;
            <(<(2, 1) | lt) | describe | println;
            <of (<(1, 2) | lt) { True => 1, False => 0 } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["yes", "no", "1"]);
}

#[test]
fn an_operation_performed_after_resume_reaches_a_handler_outside_the_resumed_code() {
    // `println` performs `IO` after `config()` has resumed; the runtime's `IO`
    // handler lies outside the `Reader` handler, so resumed code must still
    // see it.
    let dir = std::env::temp_dir().join("slc_test_effect_after_resume.sl");
    std::fs::write(
        &dir,
        r#"hook Reader { func config() -> i64; }

        func show_config() -> i64 / {Reader, IO} {
            let x = config();
            <x | println;
            x
        }

        proc main | (exit: -i32) / {IO} {
            let n = do show_config() hn {
                config(): resume => <10 | resume,
                return(v) => v,
            };
            <n | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["10", "10"]);
}

#[test]
fn a_tail_resuming_handler_runs_a_long_loop_in_constant_space() {
    // Each `tick()` resumes in tail position, so nothing is left behind per
    // iteration; a resumption that held its own machine open made this loop
    // exhaust memory.
    let dir = std::env::temp_dir().join("slc_test_many_resumptions.sl");
    std::fs::write(
        &dir,
        r#"hook Tick { func tick() -> (,); }

        func spin(n: i64) -> i64 / {Tick} {
            of (<(n, 0) | eq) {
                True => 0,
                False => { tick(); <(n, 1) | sub | spin },
            }
        }

        proc main | (exit: -i32) / {IO} {
            let done = do (<10000 | spin) hn {
                tick(): resume => <(,) | resume,
                return(v) => v,
            };
            <done | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "0");
}

#[test]
fn a_continuation_captured_in_resumed_code_continues_past_the_handler() {
    // `k` is captured after `config()` resumes and handed out as the handled
    // computation's result. Jumping to it after `handle` has returned re-enters
    // the handled code, then carries on with the program after `handle`.
    let dir = std::env::temp_dir().join("slc_test_mu_in_resumed_code.sl");
    std::fs::write(
        &dir,
        r#"hook Reader { func config() -> i64; }

        func body() -> (i64 | -i64) / {Reader} {
            let c = config();
            mu (i64 | -i64) { out <=
                <(<(mu i64 { k <= <::1(k) | out> }, c) | add) | x => ::0(x) | out>
            }
        }

        proc main | (exit: -i32) / {IO} {
            let r = do body() hn {
                config(): resume => <10 | resume,
                return(x) => x,
            };
            of r {
                ::0(n) => { <n | println; <0 | exit> },
                ::1(k) => { <"captured" | println; <32 | k> },
            }
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["captured", "42"]);
}

#[test]
fn a_mu_whose_body_performs_returns_into_every_resumption() {
    // `r` is captured under the handler / so jumping to it from a resumed copy
    // of that handler's extent lands in the copy: each resumption answers.
    let dir = std::env::temp_dir().join("slc_test_mu_under_resuming_clause.sl");
    std::fs::write(
        &dir,
        r#"hook Choose { func flip() -> Bool; }

        func pick() -> String / {Choose} {
            let a = mu String { r <= <(of flip() { True => "H", False => "T" }) | r> };
            a
        }

        proc main | (exit: -i32) / {IO} {
            let all = do pick() hn {
                flip(): resume => <((<True | resume), " ") | add | x => (x, (<False | resume)) | add,
                return(s) => s,
            };
            <all | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "H T\n");
}

#[test]
fn a_clause_cuts_into_a_continuation_it_is_handed() {
    // The clause runs below its prompt, on frames `k` shares, so the cut is a
    // jump within one extent.
    let dir = std::env::temp_dir().join("slc_test_clause_cuts_handed_continuation.sl");
    std::fs::write(
        &dir,
        r#"hook Judge { func judge(n: i64, ok: -String, bad: -String) -> (;); }

        proc main | (exit: -i32) / {IO} {
            let verdict = do (mu String { k <= <(5, k, k) | judge> }) hn {
                judge(n, ok, bad) => of (<(n, 3) | gt) { True => <"big" | ok>, False => <"small" | bad> },
                return(s) => s,
            };
            <verdict | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "big\n");
}

#[test]
fn a_continuation_jumped_to_under_a_later_handler_is_an_error() {
    // `k` is captured outside any handler of the program's own; `use_inside`
    // jumps to it from under a handler installed afterwards, a prompt `k`'s
    // stack does not hold.
    let dir = std::env::temp_dir().join("slc_test_continuation_under_later_handler.sl");
    std::fs::write(
        &dir,
        r#"hook Reader { func config() -> i64; }

        func use_inside(k: -i64) -> i64 / {Reader} {
            <config() | k>
        }

        proc main | (exit: -i32) / {IO} {
            let chosen = mu (i64 | -i64) { out <=
                <(mu i64 { k <= <::1(k) | out> }) | x => ::0(x) | out>
            };
            of chosen {
                ::0(n) => { <n | println; <0 | exit> },
                ::1(k) => {
                    let r = do (<k | use_inside) hn {
                        config(): resume => <7 | resume,
                        return(v) => v,
                    };
                    <r | println;
                    <1 | exit>
                },
            }
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(!ok, "stdout: {stdout}");
    assert!(stdout.is_empty(), "stdout: {stdout}");
    assert!(stderr.contains("left the handler it was captured under"), "stderr: {stderr}");
}

#[test]
fn a_consumer_transformer_stage_hands_on_its_answer_wherever_it_stands() {
    // `twice` reads the other way round in each chain: last in an open
    // chain, before and after forward stages, in a `let`, a tuple, a function
    // body and a handled computation, and closed on a consumer.
    let dir = std::env::temp_dir().join("slc_test_commuted_stages.sl");
    std::fs::write(
        &dir,
        r#"hook Reader { func config() -> i64; }

        func twice(out: i64) <- i64 { mu i64 { n => <(n, 2) | mul | out> } }
        func shown(out: String) <- i64 { mu i64 { n => <n | int_to_str | out> } }
        func scaled(out: i64) <- i64 / {Reader} { fn(x: i64) { <(x, config()) | mul | out> } }
        func inc(n: i64) -> i64 { <(n, 1) | add }
        func len(s: String) -> i64 { <s | str_len }
        func wrap(n: i64) -> i64 { <n | twice }

        proc main | (exit: i32) / {IO} {
            let a = <50 | twice;
            <a | println;
            <50 | twice | inc | inc | println;
            <5 | inc | twice | inc | println;
            <5 | twice | inc | twice | println;
            <12345 | shown | len | println;
            <((<50 | twice), 1) | add | println;
            <21 | wrap | println;
            let h = do (<6 | scaled) hn { config(): resume => <7 | resume, };
            <h | println;
            <mu i64 { k <= <50 | twice | inc | k> } | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        ["100", "102", "13", "22", "5", "101", "42", "42", "101"]
    );
}

#[test]
fn a_negative_trait_method_stage_consumes_what_flows_in() {
    let dir = std::env::temp_dir().join("slc_test_negative_method_stage.sl");
    std::fs::write(
        &dir,
        r#"spec Deliver { func deliver(out: String) <- Self; }

        impl Deliver for i64 {
            func deliver(out: String) <- i64 { fn(n: i64) { <("the number ", (<n | fmt)) | add | out> } }
        }

        impl Deliver for Bool {
            func deliver(out: String) <- Bool { fn(b: Bool) { <of b { True => "yes", False => "no" } | out> } }
        }

        proc main | (exit: i32) / {IO} {
            <42 | deliver | println;
            let b = <True | deliver;
            <b | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["the number 42", "yes"]);
}

#[test]
fn a_continuation_captured_outside_a_reset_cannot_be_jumped_to_inside_it() {
    // Without the `reset`, the jump to `out` answers 5; with it, the jump
    // would leave the `reset`, which is refused.
    let program = |body: &str| {
        format!(
            r#"func escape(k: -i64) -> i64 {{ <5 | k> }}

            proc main | (exit: -i32) / {{IO}} {{
                let n = mu i64 {{ out <= <({body}) | out> }};
                <n | println;
                <0 | exit>
            }}"#
        )
    };
    let free = std::env::temp_dir().join("slc_test_jump_without_reset.sl");
    std::fs::write(&free, program("<out | escape")).unwrap();
    let (stdout, stderr, ok) = run_sl(free.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "5\n");

    let barred = std::env::temp_dir().join("slc_test_jump_out_of_reset.sl");
    std::fs::write(&barred, program("reset <out | escape")).unwrap();
    let (stdout, stderr, ok) = run_sl(barred.to_str().unwrap());
    assert!(!ok, "stdout: {stdout}");
    assert!(stderr.contains("left the handler it was captured under"), "stderr: {stderr}");
}

#[test]
fn an_operation_performed_inside_a_reset_reaches_the_handler_outside_it() {
    let dir = std::env::temp_dir().join("slc_test_operation_through_reset.sl");
    std::fs::write(
        &dir,
        r#"hook Reader { func config() -> i64; }

        func read_twice() -> i64 / {Reader} { <(config(), config()) | add }

        proc main | (exit: -i32) / {IO} {
            let n = do (reset read_twice()) hn {
                config(): resume => <21 | resume,
            };
            <n | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn a_resumption_whose_slice_crosses_a_reset_reinstates_it() {
    // `r` is captured under the `reset`, and `flip` is answered outside it,
    // so each resumption carries a copy of the `reset`: the jump to `r` lands
    // on that copy, and both answers come back.
    let dir = std::env::temp_dir().join("slc_test_resumption_crosses_reset.sl");
    std::fs::write(
        &dir,
        r#"hook Choose { func flip() -> Bool; }

        func pick() -> String / {Choose} {
            reset {
                let a = mu String { r <= <(of flip() { True => "H", False => "T" }) | r> };
                a
            }
        }

        proc main | (exit: -i32) / {IO} {
            let all = do pick() hn {
                flip(): resume => <((<True | resume), " ") | add | x => (x, (<False | resume)) | add,
            };
            <all | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let (stdout, stderr, ok) = run_sl(dir.to_str().unwrap());
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "H T\n");
}
