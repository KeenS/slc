use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_handler_values_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(path)
        .output()
        .expect("failed to run slc");
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

const READER: &str = "effect Reader { fn config() -> i64; }
    fn scaled(value: i64) -> i64 / {Reader} { <(value, config()) | mul }";

#[test]
fn handler_values_handle_effectful_and_pure_computations() {
    let (success, stdout, stderr) = run(
        "basic",
        &format!(
            "{READER}
            command main | (exit: i32) / {{IO}} {{
                let reader = handler Reader {{ config(): resume => <10 | resume }};
                <(with reader handle (<7 | scaled)) | println;
                <(with reader handle 3) | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "70\n3\n");
}

#[test]
fn handler_values_can_be_stored_selected_and_composed() {
    let (success, stdout, stderr) = run(
        "stored",
        &format!(
            "{READER}
            effect Offset {{ fn offset() -> i64; }}
            fn run(reader: Handler<i64, i64, {{Reader}}, {{}}>) -> i64 {{
                with reader handle (<7 | scaled)
            }}
            command main | (exit: i32) / {{IO}} {{
                let first = handler Reader {{ config(): resume => <10 | resume }};
                let second = handler Reader {{ config(): resume => <20 | resume }};
                let choices = list::List::Cons(first, list::List::Cons(second, list::List::Nil));
                let chosen = <(choices, 1) | list::nth | (
                    fn(value: Handler<i64, i64, {{Reader}}, {{}}>) {{ value }}
                    & fn(reason: String) {{ first }}
                );
                <chosen | run | println;
                let extra = handler Offset {{ offset(): resume => <2 | resume }};
                <(with first handle (with extra handle <(<7 | scaled, offset()) | add)) | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "140\n72\n");
}

#[test]
fn stored_return_clauses_change_the_answer_even_for_a_pure_body() {
    let (success, stdout, stderr) = run(
        "return",
        &format!(
            "{READER}
            command main | (exit: i32) / {{IO}} {{
                let reader: Handler<i64, String, {{Reader}}, {{}}> = handler Reader {{
                    config(): resume => <10 | resume,
                    return(value) => <value | to_string
                }};
                <(with reader handle 42) | println;
                <(with reader handle (<7 | scaled)) | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "42\n70\n");
}

#[test]
fn stored_handler_clauses_still_require_common_answers_and_complete_coverage() {
    for (name, definition, message) in [
        ("coverage", "handler Pair { first(): resume => <1 | resume }", "second"),
        (
            "answer",
            "handler Reader { config() => 1, return(value) => \"answer\" }",
            "handler clause",
        ),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "{READER}
                effect Pair {{ fn first() -> i64; fn second() -> i64; }}
                command main | (exit: i32) / {{IO}} {{ let reader = {definition}; <0 | exit> }}"
            ),
        );
        assert!(!success, "{name}: invalid handler accepted");
        assert!(stderr.contains(message), "{name}: {stderr}");
    }
}

#[test]
fn handler_capabilities_cannot_be_widened_by_an_annotation() {
    let (success, _, stderr) = run(
        "capability",
        &format!(
            "{READER}
            effect Other {{ fn other() -> i64; }}
            command main | (exit: i32) / {{IO}} {{
                let reader: Handler<i64, i64, {{Reader, Other}}, {{}}> =
                    handler Reader {{ config(): resume => <10 | resume }};
                <(with reader handle other()) | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(!success, "a Reader handler was allowed to claim Other");
    assert!(stderr.contains("Other"), "{stderr}");
}

#[test]
fn stored_forwarding_handlers_delegate_to_the_installation_context() {
    let (success, stdout, stderr) = run(
        "forwarding",
        "effect Pair { fn first() -> i64; fn second() -> i64; }
        command main | (exit: i32) / {IO} {
            let partial = handler Pair { first(): resume => <7 | resume, _ => forward };
            let complete = handler Pair {
                first(): resume => <100 | resume,
                second(): resume => <35 | resume
            };
            let result = with complete handle (with partial handle <(first(), second()) | add);
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn handler_clause_effects_run_at_installation_not_construction() {
    let (success, stdout, stderr) = run(
        "clause_effects",
        &format!(
            "{READER}
            effect Other {{ fn other() -> i64; }}
            command main | (exit: i32) / {{IO}} {{
                let reader = handler Reader {{ config(): resume => <other() | resume }};
                <\"stored\" | println;
                let result = handle (with reader handle (<7 | scaled)) {{
                    other(): resume => {{ <\"other\" | println; <6 | resume }}
                }};
                <result | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "stored\nother\n42\n");
}
