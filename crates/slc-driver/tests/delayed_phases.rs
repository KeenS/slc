use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_delayed_phases_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

const BUILD: &str = "hook Build { func build() -> i64; }
    hook Use { func use_value(input: i64) -> i64; }
    func make() -> (i64 -> i64 / {Use}) / {Build} {
        let offset = build();
        fn(input: i64) { <(input, offset) | add | use_value }
    }";

#[test]
fn eager_alias_forces_only_construction_without_caching_the_original() {
    let (success, stdout, stderr) = run(
        "eager_alias",
        &format!(
            "{BUILD}
            data Holder {{ callback: (-> (i64 -> i64 / {{Use}}) / {{Build}}) }}
            proc main | (exit: i32) / {{IO}} {{
                let saved = Holder {{ callback: make() }};
                let pending = saved.callback;
                let alias = pending;
                let+ ready = do {{ let+ result = alias; result }} hn {{
                    build(): resume => {{ <\"build now\" | println; <10 | resume }}
                }};
                <\"ready\" | println;
                let first = do (<1 | ready) hn {{
                    use_value(input): resume => {{ <\"use\" | println; <input | resume }}
                }};
                <first | println;
                let second = do (<2 | ready) hn {{
                    use_value(input): resume => {{ <\"use\" | println; <input | resume }}
                }};
                <second | println;
                let third = do (<3 | pending) hn {{
                    build(): resume => {{ <\"build again\" | println; <20 | resume }},
                    use_value(input): resume => {{ <\"use\" | println; <input | resume }}
                }};
                <third | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "build now\nready\nuse\n11\nuse\n12\nbuild again\nuse\n23\n");
}

#[test]
fn lazy_can_return_an_effectful_function_without_activating_it() {
    let (success, stdout, stderr) = run(
        "lazy_function",
        &format!(
            "cite lazy::Lazy;
            {BUILD}
            func source() -> Lazy<(i64 -> i64 / {{Use}}), {{Build}}> {{
                mu Lazy<(i64 -> i64 / {{Use}}), {{Build}}> {{
                    force <= {{ let+ built = make(); <built | force> }}
                }}
            }}
            proc main | (exit: i32) / {{IO}} {{
                let pending = source();
                let+ ready = do pending.force hn {{
                    build(): resume => {{ <\"build\" | println; <10 | resume }}
                }};
                <\"ready\" | println;
                let first = do (<1 | ready) hn {{
                    use_value(input): resume => {{ <\"use\" | println; <input | resume }}
                }};
                <first | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "build\nready\nuse\n11\n");
}

#[test]
fn forcing_and_activation_cannot_erase_each_others_effects() {
    for (name, body, effect) in [
        (
            "missing_build",
            "let- pending: (-> (i64 -> i64 / {Use}) / {Build}) = make();
             let+ ready = pending; <0 | exit>",
            "Build",
        ),
        (
            "missing_use",
            "let- pending: (-> (i64 -> i64 / {Use}) / {Build}) = make();
             let+ ready = do { let+ result = pending; result } hn {
                 build(): resume => <10 | resume
             };
             <1 | ready | println; <0 | exit>",
            "Use",
        ),
        ("rowless_slot", "let- pending: (i64 -> i64 / {Build, Use}) = make(); <0 | exit>", "Build"),
        (
            "missing_lazy_use",
            "let pending = mu lazy::Lazy<(i64 -> i64 / {Use}), {Build}> {
                 force <= { let+ built = make(); <built | force> }
             };
             let+ ready = do pending.force hn { build(): resume => <10 | resume };
             <1 | ready | println; <0 | exit>",
            "Use",
        ),
    ] {
        let (success, _, stderr) =
            run(name, &format!("{BUILD} proc main | (exit: i32) / {{IO}} {{ {body} }}"));
        assert!(!success, "{name} unexpectedly accepted");
        assert!(stderr.contains("effect:") && stderr.contains(effect), "{name}: {stderr}");
    }
}

#[test]
fn positive_delayed_payloads_and_incorrect_row_kinds_are_rejected() {
    for (name, annotation, diagnostic) in [
        ("positive", "(-> i64 / {Build})", "positive"),
        ("row_kind", "(-> (i64 -> i64) / i64)", "row"),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "{BUILD} func invalid(value: {annotation}) -> i64 {{ 0 }}
                 proc main | (exit: i32) {{ <0 | exit> }}"
            ),
        );
        assert!(!success, "{name} unexpectedly accepted");
        assert!(stderr.contains(diagnostic), "{name}: {stderr}");
    }
}

#[test]
fn positive_lazy_demands_repeat_under_the_current_handler() {
    let (success, stdout, stderr) = run(
        "lazy_positive",
        "cite lazy::Lazy;
         hook Build { func build() -> i64; }
         proc main | (exit: i32) / {IO} {
             let pending = mu Lazy<i64, {Build}> { force <= <build() | force> };
             let+ same_menu = pending;
             <\"menu ready\" | println;
             let first = do same_menu.force hn {
                 build(): resume => { <\"first\" | println; <10 | resume }
             };
             let second = do same_menu.force hn {
                 build(): resume => { <\"second\" | println; <20 | resume }
             };
             <first | println; <second | println; <0 | exit>
         }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "menu ready\nfirst\nsecond\n10\n20\n");
}

#[test]
fn conversions_preserve_repeated_demands_and_separate_rows() {
    let (success, stdout, stderr) = run(
        "conversions",
        &format!(
            "{BUILD}
            proc main | (exit: i32) / {{IO}} {{
                let- original = make();
                let thunk = <original | lazy::of_delayed;
                let again = <thunk | lazy::to_delayed;
                <\"stored\" | println;
                let first = do (<1 | again) hn {{
                    build(): resume => {{ <\"build one\" | println; <10 | resume }},
                    use_value(input): resume => <input | resume
                }};
                <first | println;
                let second = do (<2 | again) hn {{
                    build(): resume => {{ <\"build two\" | println; <20 | resume }},
                    use_value(input): resume => <input | resume
                }};
                <second | println;
                let+ ready = do {{ let+ value = again; value }} hn {{
                    build(): resume => {{ <\"build now\" | println; <30 | resume }}
                }};
                let third = do (<3 | ready) hn {{ use_value(input): resume => <input | resume }};
                <third | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "stored\nbuild one\n11\nbuild two\n22\nbuild now\n33\n");
}

#[test]
fn a_multi_shot_build_reinstates_eager_forcing() {
    let (success, stdout, stderr) = run(
        "multi_shot",
        &format!(
            "{BUILD}
            proc main | (exit: i32) / {{IO}} {{
                let- pending = make();
                let result = do (do {{
                    let+ ready = pending;
                    <\"ready\" | println;
                    <1 | ready
                }} hn {{
                    build(): resume => <(<10 | resume, <20 | resume) | add
                }}) hn {{ use_value(input): resume => <input | resume }};
                <result | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "ready\nready\n32\n");
}

#[test]
fn eagerly_forcing_a_delayed_menu_does_not_demand_its_item() {
    let (success, stdout, stderr) = run(
        "eager_menu",
        "cite lazy::Lazy;
         hook Build { func build() -> i64; }
         hook Use { func use_value(input: i64) -> i64; }
         func make_menu() -> Lazy<i64, {Use}> / {Build} {
             let offset = build();
             mu Lazy<i64, {Use}> { force <= <offset | use_value | force> }
         }
         proc main | (exit: i32) / {IO} {
             let- pending: (-> Lazy<i64, {Use}> / {Build}) = make_menu();
             let+ ready = do { let+ value = pending; value } hn {
                 build(): resume => { <\"build\" | println; <10 | resume }
             };
             <\"menu ready\" | println;
             let first = do ready.force hn {
                 use_value(input): resume => { <\"use one\" | println; <input | resume }
             };
             let second = do ready.force hn {
                 use_value(input): resume => { <\"use two\" | println; <input | resume }
             };
             <first | println; <second | println; <0 | exit>
         }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "build\nmenu ready\nuse one\nuse two\n10\n10\n");
}

#[test]
fn unrestricted_type_parameters_accept_both_polarities_but_not_assumptions() {
    let (success, stdout, stderr) = run(
        "any_polarity",
        "func identity<*T>(value: T) -> T { value }
         func increment(value: i64) -> i64 { <(value, 1) | add }
         proc main | (exit: i32) / {IO} {
             <42 | identity | println;
             <41 | (<increment | identity) | println;
             <0 | exit>
         }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\n42\n");
    let (success, _, stderr) = run(
        "any_not_positive",
        "func positive<+T>(value: T) -> T { value }
         func invalid<*T>(value: T) -> T { <value | positive }
         proc main | (exit: i32) { <0 | exit> }",
    );
    assert!(!success, "an unrestricted type was assumed positive");
    assert!(stderr.contains("polarity") || stderr.contains("positive"), "{stderr}");
}

#[test]
fn turning_an_adapter_cannot_erase_delayed_forcing_effects() {
    for (name, annotation) in
        [("pure_adapter", "(String ; -i64)"), ("delayed_adapter", "(-> (String ; -i64) / {Build})")]
    {
        let (success, _, stderr) = run(
            name,
            &format!(
                "hook Build {{ func build() -> i64; }}
                 data Holder {{ callback: {annotation} }}
                 func make() -> (i64 -> String) / {{Build}} {{
                     let offset = build(); fn(input: i64) {{ \"answer\" }}
                 }}
                 proc main | (exit: i32) {{
                     let saved = Holder {{ callback: make() }}; <0 | exit>
                 }}"
            ),
        );
        assert!(!success, "{name} accepted the other orientation");
        assert!(stderr.contains("type:") || stderr.contains("effect:"), "{name}: {stderr}");
    }
}
