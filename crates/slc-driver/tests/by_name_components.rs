use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_by_name_components_{name}.sl"));
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
    data Holder { callback: Delayed<(i64 -> i64), {Build}> }
    enum Wrapped { Wrap(Delayed<(i64 -> i64), {Build}>) }
    func make() -> (i64 -> i64) / {Build} {
        let offset = build();
        fn(input: i64) { <(input, offset) | add }
    }";

const NEGATIVE_COMPONENTS: &[(&str, &str)] = &[
    ("tuple", "let saved = (make(), 0); let callback = saved.0;"),
    (
        "record_projection",
        "let saved = Holder { callback: make() }; let callback = saved.callback;",
    ),
    ("record_pattern", "let saved = Holder { callback: make() }; let Holder { callback } = saved;"),
    ("record_eager", "let+ saved = Holder { callback: make() }; let callback = saved.callback;"),
    ("variant", "let saved = Wrap(make()); let Wrap(callback) = saved;"),
    (
        "choice",
        "let saved: (Delayed<(i64 -> i64), {Build}> | i64) = ::0(make());
        let callback = of saved {
            ::0(value) => value,
            ::1(value) => fn(input: i64) { input }
        };",
    ),
    ("bundle", "let+ saved = (make() & fn(input: i64) { input }); let callback = saved.0;"),
    ("delayed_bundle", "let saved = (make() & fn(input: i64) { input }); let callback = saved.0;"),
];

#[test]
fn discarded_negative_components_do_not_run() {
    for (name, setup) in NEGATIVE_COMPONENTS {
        let (success, stdout, stderr) = run(
            &format!("discard_{name}"),
            &format!(
                "{BUILD} proc main | (exit: i32) / {{IO}} {{
                    {setup}
                    <\"stored\" | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "stored\n", "{name}");
    }
}

#[test]
fn negative_components_repeat_under_each_demand_handler() {
    for (name, setup) in NEGATIVE_COMPONENTS {
        let (success, stdout, stderr) = run(
            &format!("repeat_{name}"),
            &format!(
                "{BUILD} proc main | (exit: i32) / {{IO}} {{
                    let saved_callback = do {{ {setup} callback }} {{
                        build(): resume => {{ <\"wrong handler\" | println; <1000 | resume }}
                    }};
                    let alias = saved_callback;
                    <\"stored\" | println;
                    let first = do (<1 | alias) {{
                        build(): resume => {{ <\"first demand\" | println; <10 | resume }}
                    }};
                    <first | println;
                    let second = do (<2 | alias) {{
                        build(): resume => {{ <\"second demand\" | println; <20 | resume }}
                    }};
                    <second | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "stored\nfirst demand\n11\nsecond demand\n22\n", "{name}");
    }
}

#[test]
fn positive_components_still_run_during_construction() {
    for (name, setup) in [
        ("tuple", "let saved = (make(), 0); let value = saved.0;"),
        ("record", "let saved = Holder { value: make() }; let value = saved.value;"),
        ("variant", "let saved = Wrap(make()); let Wrap(value) = saved;"),
        (
            "choice",
            "let saved: (i64 | String) = ::0(make());
            let value = of saved { ::0(value) => value, ::1(text) => 0 };",
        ),
        ("bundle", "let+ saved = (make() & 0); let value = saved.0;"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("positive_{name}"),
            &format!(
                "data Holder {{ value: i64 }}
                enum Wrapped {{ Wrap(i64) }}
                func make() -> i64 / {{IO}} {{ <\"made\" | println; 42 }}
                proc main | (exit: i32) / {{IO}} {{
                    {setup}
                    <\"stored\" | println;
                    <value | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "made\nstored\n42\n", "{name}");
    }
}

#[test]
fn negative_component_rows_cannot_be_erased_by_storage() {
    for (name, setup) in [
        ("record", "let saved = PureHolder { callback: make() };"),
        ("choice", "let saved: ((i64 -> i64) | i64) = ::0(make());"),
    ] {
        let (success, _, stderr) = run(
            &format!("pure_{name}"),
            &format!(
                "{BUILD}
                data PureHolder {{ callback: (i64 -> i64) }}
                proc main | (exit: i32) / {{IO}} {{
                    do {{ {setup} (,) }} {{ build(): resume => <10 | resume }};
                    <0 | exit>
                }}"
            ),
        );
        assert!(!success, "{name}: accepted a delayed Build computation in a pure slot");
        assert!(stderr.contains("Build"), "{name}: {stderr}");
    }
}

#[test]
fn construction_handlers_do_not_discharge_negative_component_rows() {
    for (name, setup) in NEGATIVE_COMPONENTS {
        let (success, _, stderr) = run(
            &format!("unhandled_{name}"),
            &format!(
                "{BUILD} proc main | (exit: i32) / {{IO}} {{
                    let callback = do {{ {setup} callback }} {{ build(): resume => <10 | resume }};
                    <1 | callback | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(!success, "{name}: the demand lost its Build effect");
        assert!(stderr.contains("performs `Build`"), "{name}: {stderr}");
    }
}

#[test]
fn multi_shot_resumptions_do_not_cache_negative_components() {
    for (name, setup) in NEGATIVE_COMPONENTS {
        let (success, stdout, stderr) = run(
            &format!("multi_shot_{name}"),
            &format!(
                "{BUILD}
                hook Fork {{ func fork() -> i64; }}
                proc main | (exit: i32) / {{IO}} {{
                    let result = do (do {{
                        {setup}
                        let input = fork();
                        <input | callback
                    }} {{ fork(): resume => <(<1 | resume, <2 | resume) | add }}) {{
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
fn projecting_a_delayed_bundle_repeats_construction_under_the_demand_handler() {
    for (name, setup) in [
        ("literal", "let saved = (build() & 0 & 2);"),
        ("returned", "let saved = make_bundle();"),
        ("aliases", "let original = make_bundle(); let- pending = original; let saved = pending;"),
        ("nested", "let outer = (0 & (build() & 0 & 2)); let saved = outer.1;"),
        (
            "stored",
            "let holder = BundleHolder { bundle: make_bundle() }; let saved = holder.bundle;",
        ),
    ] {
        let (success, stdout, stderr) = run(
            &format!("project_bundle_{name}"),
            &format!(
                "hook Build {{ func build() -> i64; }}
                data BundleHolder {{ bundle: Delayed<(i64 & i64 & i64), {{Build}}> }}
                func make_bundle() -> (i64 & i64 & i64) / {{Build}} {{ (build() & 0 & 2) }}
                proc main | (exit: i32) / {{IO}} {{
                    {setup}
                    <\"stored\" | println;
                    let first = do saved.0 {{
                        build(): resume => {{ <\"first build\" | println; <10 | resume }}
                    }};
                    <first | println;
                    let second = do saved.2 {{
                        build(): resume => {{ <\"second build\" | println; <20 | resume }}
                    }};
                    <second | println;
                    <0 | exit>
                }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert!(stderr.is_empty(), "{name}: {stderr}");
        assert_eq!(stdout, "stored\nfirst build\n10\nsecond build\n2\n", "{name}");
    }
}

#[test]
fn projecting_a_delayed_bundle_does_not_force_its_negative_item() {
    let (success, stdout, stderr) = run(
        "project_without_activating",
        "hook Build { func build() -> i64; }
        hook Use { func use_value() -> i64; }
        func make_callback() -> (i64 -> i64) / {Use} {
            let offset = use_value();
            fn(input: i64) { <(input, offset) | add }
        }
        func make_bundle() -> (Delayed<(i64 -> i64), {Use}> & i64) / {Build} {
            let value = build();
            (make_callback() & value)
        }
        proc main | (exit: i32) / {IO} {
            let pending = make_bundle();
            do {
                pending.0;
                <\"projected, not activated\" | println;
                (,)
            } { build(): resume => { <\"build\" | println; <10 | resume } };
            let result = do (do (<1 | pending.0) {
                build(): resume => { <\"build again\" | println; <20 | resume }
            }) { use_value(): resume => { <\"use\" | println; <30 | resume } };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "build\nprojected, not activated\nbuild again\nuse\n31\n");
}

#[test]
fn delayed_bundle_projection_retains_construction_and_item_effects() {
    for (name, expression, effect) in [
        ("construction", "pending.0", "Build"),
        ("item", "do (<1 | pending.0) { build(): resume => <10 | resume }", "Use"),
    ] {
        let (success, _, stderr) = run(
            &format!("unhandled_bundle_{name}"),
            &format!(
                "hook Build {{ func build() -> i64; }}
                hook Use {{ func use_value() -> i64; }}
                func make_bundle() -> ((i64 -> i64 / {{Use}}) & i64) / {{Build}} {{
                    let value = build();
                    (fn(input: i64) {{ use_value() }} & value)
                }}
                proc main | (exit: i32) / {{IO}} {{
                    let pending = make_bundle();
                    {expression};
                    <0 | exit>
                }}"
            ),
        );
        assert!(!success, "{name}: accepted an unhandled {effect} effect");
        assert!(stderr.contains(&format!("performs `{effect}`")), "{name}: {stderr}");
    }
}

#[test]
fn resuming_bundle_construction_reinstates_the_pending_projection() {
    let (success, stdout, stderr) = run(
        "resume_bundle_projection",
        "hook Build { func build() -> i64; }
        func make_bundle() -> (i64 & i64) / {Build} {
            let value = build();
            (value & <(value, 2) | mul)
        }
        proc main | (exit: i32) / {IO} {
            let pending = make_bundle();
            let result = do pending.1 {
                build(): resume => <(<3 | resume, <4 | resume) | add
            };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "14\n");
}
