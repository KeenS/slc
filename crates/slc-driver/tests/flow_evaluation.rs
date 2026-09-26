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

const BUILD: &str = "hook Build { func build() -> i64; }
    func make(input: i64) -> (i64 -> i64) / {Build} {
        let offset = build();
        fn(value: i64) { <(value, offset) | add }
    }
    func ignore(callback: (-> (i64 -> i64) / {Build})) -> i64 { 0 }
    func twice(callback: (-> (i64 -> i64) / {Build})) -> i64 / {Build} {
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
                proc main | (exit: i32) / {{IO}} {{
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
                proc main | (exit: i32) / {{IO}} {{
                    let result = do ({expression}) hn {{
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
                proc send(callback: (-> (i64 -> i64) / {{Build}})) | (answer: i64) / {{Build}} {{
                    <(<callback | twice) | answer>
                }}
                proc main | (exit: i32) / {{IO}} {{
                    let result = do (mu i64 {{ answer <= {expression} }}) hn {{
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
                func locally_twice(callback: (-> (i64 -> i64) / {{Build}})) -> i64 / {{IO}} {{
                    do (<callback | twice) hn {{
                        build(): resume => {{ <\"callee build\" | println; <10 | resume }}
                    }}
                }}
                proc main | (exit: i32) / {{IO}} {{
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
                "func first(value: i64) -> i64 / {{IO}} {{ <\"first\" | println; value }}
                func second(value: i64) -> i64 / {{IO}} {{ <\"second\" | println; value }}
                proc main | (exit: i32) / {{IO}} {{
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
                "func input() -> i64 / {{IO}} {{ <\"input\" | println; 1 }}
                func first() -> (i64 -> i64) / {{IO}} {{
                    <\"first stage\" | println; fn(value: i64) {{ value }}
                }}
                func second() -> (i64 -> i64) / {{IO}} {{
                    <\"second stage\" | println; fn(value: i64) {{ value }}
                }}
                proc main | (exit: i32) / {{IO}} {{
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
                "func input() -> i64 / {{IO}} {{ <\"input\" | println; 42 }}
                 func identity(value: i64) -> i64 {{ value }}
                 func sink(exit: -i32) -> (-i64 / {{IO}}) / {{IO}} {{
                     <\"consumer\" | println;
                     mu i64 {{ value => {{ <value | println; <0 | exit> }} }}
                 }}
                 proc main | (exit: i32) / {{IO}} {{ {expression} }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert_eq!(stdout, "input\nconsumer\n42\n", "{name}");
    }
}
