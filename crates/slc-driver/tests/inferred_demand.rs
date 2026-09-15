use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_inferred_demand_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slc"))
        .args(["run", path.to_str().unwrap()])
        .output()
        .expect("failed to run slc");
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

const BUILD: &str = "effect Build { fn build() -> i64; }
    fn ignore(callback: Delayed<(i64 -> i64), {Build}>) -> i64 { 0 }
    fn twice(callback: Delayed<(i64 -> i64), {Build}>) -> i64 / {Build} {
        <(<1 | callback, <2 | callback) | add
    }";

#[test]
fn inferred_negative_intermediates_do_not_run_when_discarded() {
    for (name, body) in [
        ("flow", "<{ build(); value } | ignore"),
        ("binding", "let pending = { build(); value }; <pending | ignore"),
        ("tuple", "let saved = ({ build(); value }, 0); <saved.0 | ignore"),
        ("record", "let saved = Holder { callback: { build(); value } }; <saved.callback | ignore"),
    ] {
        let (success, stdout, stderr) = run(
            name,
            &format!(
                "{BUILD}
                 data Holder {{ callback: Delayed<(i64 -> i64), {{Build}}> }}
                 command main | (exit: i32) / {{IO}} {{
                     let discard = fn(value) {{ {body} }};
                     <fn(input: i64) {{ input }} | discard | println;
                     <0 | exit>
                 }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert_eq!(stdout, "0\n", "{name}");
    }
}

#[test]
fn inferred_negative_effects_run_under_the_demand_handler() {
    let (success, stdout, stderr) = run(
        "demand_handler",
        &format!(
            "{BUILD}
             command main | (exit: i32) / {{IO}} {{
                 let use_twice = fn(value) {{
                     let stored = handle {{ let pending = {{ build(); value }}; pending }} {{
                         build(): resume => <0 | resume
                     }};
                     handle (<stored | twice) {{
                         build(): resume => {{ <\"demand\" | println; <0 | resume }}
                     }}
                 }};
                 <fn(input: i64) {{ input }} | use_twice | println;
                 <0 | exit>
             }}"
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "demand\ndemand\n3\n");
}

#[test]
fn construction_handlers_cannot_erase_inferred_latent_effects() {
    let (success, _, stderr) = run(
        "escaping",
        &format!(
            "{BUILD}
             command main | (exit: i32) / {{IO}} {{
                 let use_twice = fn(value) {{
                     let stored = handle {{ let pending = {{ build(); value }}; pending }} {{
                         build(): resume => <0 | resume
                     }};
                     <stored | twice
                 }};
                 <fn(input: i64) {{ input }} | use_twice | println;
                 <0 | exit>
             }}"
        ),
    );
    assert!(!success, "an inferred delayed effect escaped its handler");
    assert!(stderr.contains("Build"), "{stderr}");
}

#[test]
fn inferred_positive_intermediates_still_compute_immediately() {
    let (success, stdout, stderr) = run(
        "positive",
        "fn ignore(value: i64) -> i64 { 0 }
         command main | (exit: i32) / {IO} {
             let discard = fn(value) { <{ <\"computed\" | println; value } | ignore };
             <42 | discard | println;
             <0 | exit>
         }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "computed\n0\n");
}

#[test]
fn primitive_calls_follow_the_same_argument_discipline_as_flows_and_aliases() {
    for (name, expression) in [
        ("direct", "__display(make())"),
        ("flow", "<make() | __display"),
        ("alias", "<make() | primitive"),
    ] {
        let (success, stdout, stderr) = run(
            &format!("primitive_{name}"),
            &format!(
                "fn make() -> (i64 -> i64) / {{IO}} {{
                    <\"constructed\" | println; fn(input: i64) {{ input }}
                 }}
                 command main | (exit: i32) / {{IO}} {{
                     let primitive = __display;
                     <({expression}) | println;
                     <0 | exit>
                 }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert_eq!(stdout, "<delayed>\n", "{name}");
    }
}

#[test]
fn nullary_function_names_are_values_and_calls_are_explicit() {
    let (success, stdout, stderr) = run(
        "nullary",
        "fn make() -> i64 / {IO} { <\"called\" | println; 42 }
         fn invoke(factory: ((,) -> i64 / {IO})) -> i64 / {IO} { <(,) | factory }
         command main | (exit: i32) / {IO} {
             let factory = make;
             <\"stored\" | println;
             <factory | invoke | println;
             <make() | println;
             <(,) | make | println;
             <0 | exit>
         }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "stored\ncalled\n42\ncalled\n42\ncalled\n42\n");
}

#[test]
fn primitive_outcomes_force_only_the_selected_callback() {
    for (name, expression) in [
        (
            "direct",
            "parse_int(\"42\", { <\"selected\" | println; success },
                { <\"unselected\" | println; failure }, failure)",
        ),
        (
            "flow",
            "<\"42\" | parse_int | ({ <\"selected\" | println; success } &
                { <\"unselected\" | println; failure } & failure)>",
        ),
    ] {
        let (success, stdout, stderr) = run(
            &format!("outcome_{name}"),
            &format!(
                "command main | (exit: i32) / {{IO}} {{
                     let success = select i64 {{ value => {{ <value | println; <0 | exit> }} }};
                     let failure = select String {{ message => {{ <message | println; <1 | exit> }} }};
                     {expression}
                 }}"
            ),
        );
        assert!(success, "{name}: {stderr}");
        assert_eq!(stdout, "selected\n42\n", "{name}");
    }
}
