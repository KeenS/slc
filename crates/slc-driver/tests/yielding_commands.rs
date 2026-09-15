use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_yielding_commands_{name}.sl"));
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

const CHOOSE: &str = "command choose<E>(input: i64) |
    (positive: (-i64 / {..E}) & negative: (-i64 / {..E})) / {..E} {
        match (<(input, 0) | gt) {
            True => <input | positive>,
            False => <input | negative>
        }
    }";

#[test]
fn returning_exits_yield_the_selected_result_and_compose_on() {
    let (success, stdout, stderr) = run(
        "results",
        &format!(
            "{CHOOSE}
            command single(input: i64) | (answer: i64) {{ <input | answer> }}
            command main | (exit: i32) / {{IO}} {{
                <4 | choose | (fn(value: i64) {{ <(value, 10) | add }} & fn(value: i64) {{ 0 }}) | println;
                <-4 | choose | (fn(value: i64) {{ 0 }} & fn(value: i64) {{ <(value, 10) | sub }}) | println;
                <7 | single | fn(value: i64) {{ <(value, 2) | mul }} | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "14\n-14\n14\n");
}

#[test]
fn returning_exits_preserve_effects_and_run_only_the_selected_callback() {
    let (success, stdout, stderr) = run(
        "selected_effect",
        &format!(
            "{CHOOSE}
            effect Read {{ fn read() -> i64; }}
            fn callback(input: i64) -> i64 / {{Read}} {{ <(input, read()) | add }}
            command main | (exit: i32) / {{IO}} {{
                let value = handle (<2 | choose | (callback & fn(input: i64) {{ <100 | callback }})) {{
                    read(): resume => {{ <\"read\" | println; <40 | resume }}
                }};
                <value | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "read\n42\n");
}

#[test]
fn returning_exits_reject_mixed_callbacks_and_incompatible_answers() {
    for (name, exits, message) in [
        ("mixed", "(fn(value: i64) { value } & select i64 { value => <0 | exit> })", "consumer"),
        ("answers", "(fn(value: i64) { value } & fn(value: i64) { \"text\" })", "result"),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "{CHOOSE}
                command main | (exit: i32) / {{IO}} {{
                    let answer = <1 | choose | {exits};
                    <0 | exit>
                }}"
            ),
        );
        assert!(!success, "{name}: invalid callbacks were accepted");
        assert!(stderr.contains(message), "{name}: {stderr}");
    }
}

#[test]
fn returning_exit_effects_cannot_escape_unhandled() {
    let (success, _, stderr) = run(
        "unhandled",
        &format!(
            "{CHOOSE}
            effect Read {{ fn read() -> i64; }}
            command main | (exit: i32) / {{IO}} {{
                let answer = <1 | choose | (fn(value: i64) {{ read() }} & fn(value: i64) {{ 0 }});
                <0 | exit>
            }}"
        ),
    );
    assert!(!success, "an unhandled callback effect was accepted");
    assert!(stderr.contains("Read"), "{stderr}");
}

#[test]
fn returning_exits_preserve_value_order_and_delay_unselected_construction() {
    let (success, stdout, stderr) = run(
        "construction_order",
        &format!(
            "{CHOOSE}
            fn input() -> i64 / {{IO}} {{ <\"input\" | println; 2 }}
            fn make_callback(label: String) -> (i64 -> i64) / {{IO}} {{
                <label | println;
                fn(value: i64) {{ value }}
            }}
            fn callbacks() -> (Delayed<(i64 -> i64), {{IO}}> & Delayed<(i64 -> i64), {{IO}}>) / {{IO}} {{
                <\"bundle\" | println;
                (<\"selected\" | make_callback & <\"unselected\" | make_callback)
            }}
            command main | (exit: i32) / {{IO}} {{
                <input() | choose | callbacks() | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "input\nbundle\nselected\n2\n");
}

#[test]
fn returning_exits_resume_into_the_remainder_of_the_chain() {
    let (success, stdout, stderr) = run(
        "multishot",
        &format!(
            "{CHOOSE}
            effect Read {{ fn read() -> i64; }}
            fn scale(value: i64) -> i64 {{ <(value, 10) | mul }}
            command main | (exit: i32) / {{IO}} {{
                let answer = handle (<1 | choose | (
                    fn(value: i64) {{ read() }} & fn(value: i64) {{ 0 }}
                ) | scale) {{ read(): resume => <(<1 | resume, <2 | resume) | add }};
                <answer | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "30\n");
}
