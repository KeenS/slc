use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_generic_effects_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slc")).arg("run").arg(path).output().unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn generic_operations_and_handlers_share_their_effect_arguments() {
    let (success, stdout, stderr) = run(
        "reader",
        "
        effect Reader<+T> { fn read() -> T; }
        fn get<+T>() -> T / {Reader<T>} { read() }
        command main | (exit: i32) / {IO} {
            let number = handle get() { read(): resume => <42 | resume };
            let text = handle get() { read(): resume => <\"hello\" | resume };
            <number | println;
            <text | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\nhello\n");
}

#[test]
fn nested_handlers_may_use_different_instantiations() {
    let (success, stdout, stderr) = run(
        "nested",
        "
        effect Reader<+T> { fn read() -> T; }
        fn number() -> i64 / {Reader<i64>} { read() }
        fn text() -> String / {Reader<String>} { read() }
        command main | (exit: i32) / {IO} {
            let result = handle {
                let inner = handle text() { read(): resume => <\"inner\" | resume };
                <inner | println;
                number()
            } { read(): resume => <42 | resume };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "inner\n42\n");
}

#[test]
fn incompatible_same_name_interception_is_rejected() {
    let (success, _, stderr) = run(
        "wrong_handler",
        "
        effect Reader<+T> { fn read() -> T; }
        fn number() -> i64 / {Reader<i64>} { read() }
        command main | (exit: i32) / {IO} {
            let result = handle {
                handle number() { read(): resume => <\"wrong\" | resume }
            } { read(): resume => <42 | resume };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(!success, "incompatible handler intercepted a typed operation");
    assert!(stderr.contains("Reader"), "{stderr}");
}

#[test]
fn generic_effect_rows_validate_names_arity_kinds_and_polarity() {
    for (name, row) in [
        ("missing", "Missing<i64>"),
        ("arity", "Reader<i64, String>"),
        ("bare", "Reader"),
        ("kind", "Reader<{}>"),
        ("polarity", "Reader<-i64>"),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "
            effect Reader<+T> {{ fn read() -> T; }}
            fn unused() -> i64 / {{{row}}} {{ 0 }}
            command main | (exit: i32) {{ <0 | exit> }}"
            ),
        );
        assert!(!success, "{name}: invalid effect row accepted");
        assert!(!stderr.is_empty());
    }
}

#[test]
fn generic_handler_values_keep_their_capabilities() {
    let (success, stdout, stderr) = run(
        "stored",
        "
        effect Reader<+T> { fn read() -> T; }
        command main | (exit: i32) / {IO} {
            let reader: Handler<i64, i64, {Reader<i64>}, {}> = handler Reader {
                read(): resume => <42 | resume
            };
            <(with reader handle read()) | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn operations_of_one_effect_share_one_handler_instantiation() {
    let (success, stdout, stderr) = run(
        "pair",
        "
        effect Pair<+T> { fn first() -> T; fn second() -> T; }
        command main | (exit: i32) / {IO} {
            let result = handle <(first(), second()) | add {
                first(): resume => <10 | resume,
                second(): resume => <32 | resume
            };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\n");
    let (success, _, stderr) = run(
        "wrong_pair",
        "
        effect Pair<+T> { fn first() -> T; fn second() -> T; }
        command main | (exit: i32) / {IO} {
            let result = handle first() {
                first(): resume => <10 | resume,
                second(): resume => <\"wrong\" | resume
            };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(!success, "mismatched clauses were accepted: {stderr}");
}

#[test]
fn composable_capture_resumes_twice_and_preserves_unrelated_effects() {
    let (success, stdout, stderr) = run(
        "capture",
        "
        effect Factor { fn factor() -> i64; }
        command main | (exit: i32) / {IO} {
            let result = handle (<fn {
                let value = <fn(resume: (i64 -> i64 / {Factor})) {
                    <(<1 | resume, <2 | resume) | add
                } | control::shift;
                <(value, factor()) | mul
            } | control::reset) { factor(): resume => <10 | resume };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "30\n");
}

#[test]
fn capture_handlers_nest_with_distinct_answer_types() {
    let (success, stdout, stderr) = run("nested_capture", "
        command main | (exit: i32) / {IO} {
            let result = <fn {
                let inner = <fn {
                    let value = <fn(resume: (String -> String)) { <\"inner\" | resume } | control::shift;
                    <(value, \"!\") | add
                } | control::reset;
                <inner | println;
                let value = <fn(resume: (i64 -> i64 / {IO})) { <2 | resume } | control::shift;
                <(value, 10) | mul
            } | control::reset;
            <result | println;
            <0 | exit>
        }");
    assert!(success, "{stderr}");
    assert_eq!(stdout, "inner!\n20\n");
}

#[test]
fn bare_reset_does_not_handle_shift_and_answers_must_agree() {
    for (name, expression, message) in [
        ("unhandled", "<fn(resume: (i64 -> i64)) { <1 | resume } | control::shift", "Shift"),
        (
            "delimiter",
            "reset (<fn(resume: (i64 -> i64)) { <1 | resume } | control::shift)",
            "Shift",
        ),
        (
            "answer",
            "<fn { let value = <fn(resume: (i64 -> String)) { <1 | resume } | control::shift; <(value, 1) | add } | control::reset",
            "Shift",
        ),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "
            command main | (exit: i32) / {{IO}} {{
                let result = {expression}; <result | println; <0 | exit>
            }}"
            ),
        );
        assert!(!success, "{name}: invalid capture accepted");
        assert!(stderr.contains(message), "{name}: {stderr}");
    }
}

#[test]
fn module_resolution_preserves_generic_scopes_and_effect_paths() {
    let (success, stdout, stderr) = run(
        "modules",
        "
        mod provider {
            pub data T { value: String }
            pub effect Reader<+T> { fn read() -> T; }
            pub fn get<+T>() -> T / {Reader<T>} { read() }
        }
        command main | (exit: i32) / {IO} {
            let result = handle provider::get() { provider::read(): resume => <42 | resume };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn capture_forces_callback_construction_without_caching_resumptions() {
    let (success, stdout, stderr) = run(
        "callback_factory",
        "
        fn make() -> ((i64 -> i64 / {IO}) -> i64 / {IO}) / {IO} {
            <\"build\" | println;
            fn(resume: (i64 -> i64 / {IO})) { <(<1 | resume, <2 | resume) | add }
        }
        command main | (exit: i32) / {IO} {
            let result = <fn {
                let value = <make() | control::shift;
                <\"resumed\" | println;
                <(value, 10) | mul
            } | control::reset;
            <result | println;
            <(<fn { 42 } | control::reset) | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "build\nresumed\nresumed\n30\n42\n");
}

#[test]
fn generic_forwarding_keeps_the_intercepted_instantiation() {
    let (success, stdout, stderr) = run(
        "forward",
        "
        effect Pair<+T> { fn first() -> T; fn second() -> T; }
        command main | (exit: i32) / {IO} {
            let result = handle {
                handle <(first(), second()) | add {
                    first(): resume => <7 | resume,
                    _ => forward
                }
            } {
                first(): resume => <100 | resume,
                second(): resume => <35 | resume
            };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn generic_effect_parameters_and_local_row_annotations_are_checked() {
    for (name, declarations, body) in [
        ("duplicate", "effect Reader<+T, +T> { fn read() -> T; }", ""),
        ("unsigned", "effect Reader<T> { fn read() -> T; }", ""),
        ("unknown", "effect Reader<+T> { fn read(value: Missing) -> T; }", ""),
        (
            "local_polarity",
            "effect Reader<+T> { fn read() -> T; }",
            "let callback: ((,) -> i64 / {Reader<-i64>}) = fn { 0 };",
        ),
        (
            "capability",
            "effect Reader<+T> { fn read() -> T; }",
            "let wrong: Handler<i64, i64, {Reader<String>}, {}> = handler Reader { read(): resume => <42 | resume };",
        ),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "
            {declarations}
            command main | (exit: i32) {{ {body} <0 | exit> }}"
            ),
        );
        assert!(!success, "{name}: invalid generic effect accepted");
        assert!(!stderr.is_empty());
    }
}

#[test]
fn generic_latent_rows_keep_demand_time_handlers() {
    let (success, stdout, stderr) = run(
        "latent",
        "
        effect Reader<+T> { fn read() -> T; }
        menu Source<+T> / {Reader<T>} { value: T }
        fn make<+T>() -> Source<T> {
            mu Source<T> { value <= <read() | value> }
        }
        command main | (exit: i32) / {IO} {
            let source: Source<i64> = make();
            <(handle source.value { read(): resume => <10 | resume }) | println;
            <(handle source.value { read(): resume => <20 | resume }) | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "10\n20\n");
}

#[test]
fn capture_cannot_erase_an_unhandled_residual_effect() {
    let (success, _, stderr) = run(
        "unhandled_residual",
        "
        effect Factor { fn factor() -> i64; }
        command main | (exit: i32) / {IO} {
            let result = <fn {
                let value = <fn(resume: (i64 -> i64 / {Factor})) { <2 | resume } | control::shift;
                <(value, factor()) | mul
            } | control::reset;
            <result | println;
            <0 | exit>
        }",
    );
    assert!(!success, "capture erased Factor");
    assert!(stderr.contains("Factor"), "{stderr}");
}
