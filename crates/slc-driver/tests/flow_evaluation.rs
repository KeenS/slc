use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_flow_evaluation_{name}.sl"));
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

const BUILD: &str = "effect Build { fn build() -> i64; }
    fn make(input: i64) -> (i64 -> i64) / {Build} {
        let offset = build();
        fn(value: i64) { <(value, offset) | add }
    }
    fn ignore(callback: Delayed<(i64 -> i64), {Build}>) -> i64 { 0 }
    fn twice(callback: Delayed<(i64 -> i64), {Build}>) -> i64 / {Build} {
        <(<1 | callback, <2 | callback) | add
    }";

#[test]
fn regrouping_discarded_negative_intermediates_performs_nothing() {
    for (name, expression) in [
        ("flat", "<1 | make | ignore"),
        ("nested", "<(<1 | make) | ignore"),
        ("binder", "<1 | make | callback => callback | ignore"),
        ("composition", "<1 | (make | ignore)"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("discard_{name}"),
            &format!(
                "{BUILD}
                command main | (exit: i32) / {{IO}} {{
                    <({expression}) | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "0\n", "{name}");
    }
}

#[test]
fn regrouping_preserves_repeated_demand_and_handler_selection() {
    for (name, expression) in [
        ("flat", "<1 | make | twice"),
        ("nested", "<(<1 | make) | twice"),
        ("binder", "<1 | make | callback => callback | twice"),
        ("composition", "<1 | (make | twice)"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("repeat_{name}"),
            &format!(
                "{BUILD}
                command main | (exit: i32) / {{IO}} {{
                    let result = handle ({expression}) {{
                        build(): resume => {{ <\"build\" | println; <10 | resume }}
                    }};
                    <result | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "build\nbuild\n23\n", "{name}");
    }
}

#[test]
fn regrouping_preserves_command_value_and_exit_groups() {
    for (name, expression) in
        [("flat", "<1 | make | send | answer>"), ("nested", "<(<1 | make) | send | answer>")]
    {
        let (success, stdout, stderr) = run(
            &format!("command_{name}"),
            &format!(
                "{BUILD}
                command send(callback: Delayed<(i64 -> i64), {{Build}}>) | (answer: i64) / {{Build}} {{
                    <(<callback | twice) | answer>
                }}
                command main | (exit: i32) / {{IO}} {{
                    let result = handle (mu i64 {{ answer <= {expression} }}) {{
                        build(): resume => {{ <\"build\" | println; <10 | resume }}
                    }};
                    <result | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "build\nbuild\n23\n", "{name}");
    }
}

#[test]
fn an_intermediate_runs_under_the_callees_handler() {
    for (name, expression) in [
        ("flat", "<1 | make | locally_twice"),
        ("nested", "<(<1 | make) | locally_twice"),
        ("composition", "<1 | (make | locally_twice)"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("callee_handler_{name}"),
            &format!(
                "{BUILD}
                fn locally_twice(callback: Delayed<(i64 -> i64), {{Build}}>) -> i64 / {{IO}} {{
                    handle (<callback | twice) {{
                        build(): resume => {{ <\"callee build\" | println; <10 | resume }}
                    }}
                }}
                command main | (exit: i32) / {{IO}} {{
                    <({expression}) | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "callee build\ncallee build\n23\n", "{name}");
    }
}

#[test]
fn regrouping_keeps_positive_computations_eager_and_ordered() {
    for (name, expression) in [
        ("flat", "<1 | first | second"),
        ("nested", "<(<1 | first) | second"),
        ("composition", "<1 | (first | second)"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("positive_{name}"),
            &format!(
                "fn first(value: i64) -> i64 / {{IO}} {{ <\"first\" | println; value }}
                fn second(value: i64) -> i64 / {{IO}} {{ <\"second\" | println; value }}
                command main | (exit: i32) / {{IO}} {{
                    <({expression}) | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "first\nsecond\n1\n", "{name}");
    }
}

#[test]
fn regrouping_preserves_the_order_of_computed_stages() {
    for (name, expression) in [
        ("flat", "<input() | first() | second()"),
        ("nested", "<(<input() | first()) | second()"),
        ("composition", "<input() | (first() | second())"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("computed_stages_{name}"),
            &format!(
                "fn input() -> i64 / {{IO}} {{ <\"input\" | println; 1 }}
                fn first() -> (i64 -> i64) / {{IO}} {{
                    <\"first stage\" | println; fn(value: i64) {{ value }}
                }}
                fn second() -> (i64 -> i64) / {{IO}} {{
                    <\"second stage\" | println; fn(value: i64) {{ value }}
                }}
                command main | (exit: i32) / {{IO}} {{
                    <({expression}) | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "input\nfirst stage\nsecond stage\n1\n", "{name}");
    }
}

#[test]
fn computed_consumers_follow_positive_inputs_in_every_grouping() {
    for (name, expression) in [
        ("direct", "<input() | (<exit | sink)>"),
        ("flat", "<input() | identity | (<exit | sink)>"),
        ("nested", "<(<input() | identity) | (<exit | sink)>"),
        ("eager", "let+ value = input(); <value | (<exit | sink)>"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("consumer_order_{name}"),
            &format!(
                "fn input() -> i64 / {{IO}} {{ <\"input\" | println; 42 }}
                 fn identity(value: i64) -> i64 {{ value }}
                 fn sink(exit: -i32) -> (-i64 / {{IO}}) / {{IO}} {{
                     <\"consumer\" | println;
                     select i64 {{ value => {{ <value | println; <0 | exit> }} }}
                 }}
                 command main | (exit: i32) / {{IO}} {{ {expression} }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert_eq!(stdout, "input\nconsumer\n42\n", "{name}");
    }
}
