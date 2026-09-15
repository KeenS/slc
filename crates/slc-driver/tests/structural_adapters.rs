use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_structural_adapters_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slc")).arg("run").arg(&path).output().unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

const STAGE: &str =
    "fn deliver(out: String) <- i64 { select i64 { number => <number | int_to_str | out> } }";

#[test]
fn stored_products_sums_records_and_recursive_variants_adapt() {
    let (success, stdout, stderr) = run(
        "structures",
        &format!(
            r#"
        {STAGE}
        data Box<-F> {{ value: F }}
        enum Chain<-F> {{ End, Link(F, Chain<F>) }}
        fn take(chain: Chain<(i64 -> String)>) -> String {{
            match chain {{ End => "end", Link(stage, rest) => <42 | stage }}
        }}
        command main | (exit: i32) / {{IO}} {{
            let original = (deliver, 1);
            let pair: ((i64 -> String), i64) = original;
            <pair.1 | pair.0 | println;
            let original: ((-String -> -i64) | i64) = ::0(deliver);
            let choice: ((i64 -> String) | i64) = original;
            match choice {{ ::0(stage) => <2 | stage | println, ::1(number) => <number | println }};
            let original = Box {{ value: deliver }};
            let boxed: Box<(i64 -> String)> = original;
            <3 | boxed.value | println;
            let chain = Chain::Link(deliver, Chain::Link(deliver, Chain::End));
            <chain | take | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "1\n2\n3\n42\n");
}

#[test]
fn higher_order_inputs_outputs_and_menu_answers_adapt() {
    let (success, stdout, stderr) = run(
        "higher_order",
        &format!(
            r#"
        {STAGE}
        data Box<-F> {{ value: F }}
        menu Provider<-F> {{ get: F }}
        fn use_stage(stage: (i64 -> String)) -> String {{ <4 | stage }}
        fn provide() -> Box<(-String -> -i64)> {{ Box {{ value: deliver }} }}
        command main | (exit: i32) / {{IO}} {{
            let use_other: ((-String -> -i64) -> String) = use_stage;
            <deliver | use_other | println;
            let make_box: ((,) -> Box<(i64 -> String)>) = provide;
            let boxed = make_box();
            <5 | boxed.value | println;
            let original: Provider<(-String -> -i64)> = mu Provider<(-String -> -i64)> {{ get: out <= <deliver | out> }};
            let provider: Provider<(i64 -> String)> = original;
            <6 | provider.get | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "4\n5\n6\n");
}

#[test]
fn delayed_turning_keeps_construction_at_each_forcing_boundary() {
    let (success, stdout, stderr) = run(
        "phases",
        r#"
        effect Build { fn build() -> i64; }
        effect Use { fn use_value(input: i64) -> i64; }
        data Box<-F> { value: F }
        fn make() -> (i64 -> i64 / {Use}) / {Build} {
            let offset = build();
            fn(input: i64) { <(input, offset) | add | use_value }
        }
        command main | (exit: i32) / {IO} {
            let original = Box { value: make() };
            let boxed: Box<Delayed<(-i64 -> -i64 / {Use}), {Build}>> = original;
            <"stored" | println;
            let+ ready = handle { let+ value = boxed.value; value } {
                build(): resume => { <"build now" | println; <10 | resume }
            };
            <"ready" | println;
            <handle (<1 | ready) { use_value(input): resume => { <"use" | println; <input | resume } } | println;
            <handle (<2 | boxed.value) {
                build(): resume => { <"build again" | println; <20 | resume },
                use_value(input): resume => { <"use" | println; <input | resume }
            } | println;
            <handle (<3 | boxed.value) {
                build(): resume => { <"build again" | println; <30 | resume },
                use_value(input): resume => { <"use" | println; <input | resume }
            } | println;
            <0 | exit>
        }
    "#,
    );
    assert!(success, "{stderr}");
    assert_eq!(
        stdout,
        "stored\nbuild now\nready\nuse\n11\nbuild again\nuse\n22\nbuild again\nuse\n33\n"
    );
}

#[test]
fn adapters_cannot_erase_forcing_or_activation_rows() {
    for (name, annotation) in
        [("forcing", "(-i64 -> -i64 / {Use})"), ("activation", "Delayed<(-i64 -> -i64), {Build}>")]
    {
        let (success, _, stderr) = run(
            name,
            &format!(
                r#"
            effect Build {{ fn build() -> i64; }}
            effect Use {{ fn use_value(input: i64) -> i64; }}
            data Box<-F> {{ value: F }}
            fn make() -> (i64 -> i64 / {{Use}}) / {{Build}} {{
                let offset = build(); fn(input: i64) {{ <input | use_value }}
            }}
            command main | (exit: i32) {{
                let original = Box {{ value: make() }};
                let boxed: Box<{annotation}> = original;
                <0 | exit>
            }}
        "#
            ),
        );
        assert!(!success, "{name} accepted an erased row");
        assert!(stderr.contains("effect:"), "{stderr}");
    }
}

#[test]
fn dual_parameter_occurrences_and_consumers_adapt_contravariantly() {
    let (success, stdout, stderr) = run(
        "duals",
        &format!(
            r#"
        {STAGE}
        data Box<-F> {{ value: F }}
        data Request<-F> {{ value: -F }}
        command main | (exit: i32) / {{IO}} {{
            let number = mu String {{ out <= {{
                let consumer: -Box<(-String -> -i64)> = select Box<(i64 -> String)> {{ Box {{ value: stage }} => <7 | stage | out> }};
                <Box {{ value: deliver }} | consumer>
            }} }};
            <number | println;
            let text = mu String {{ out <= {{
                let request: Request<(i64 -> String)> = Request {{ value: (8, out) }};
                let reversed: Request<(-String -> -i64)> = request;
                <reversed.value.1 | int_to_str | reversed.value.0>
            }} }};
            <text | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "7\n8\n");
}

#[test]
fn double_turning_and_alternative_paths_agree() {
    let (success, stdout, stderr) = run(
        "round_trip",
        &format!(
            r#"
        {STAGE}
        data Box<-F> {{ value: F }}
        command main | (exit: i32) / {{IO}} {{
            let original = Box {{ value: deliver }};
            let forward: Box<(i64 -> String)> = original;
            let backward: Box<(-String -> -i64)> = forward;
            let again: Box<(i64 -> String)> = backward;
            let stage: (i64 -> String) = deliver;
            let direct = Box {{ value: stage }};
            <9 | forward.value | println;
            <9 | again.value | println;
            <9 | direct.value | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "9\n9\n9\n");
}

#[test]
fn recursive_menu_lifting_leaves_unrequested_answers_unrun() {
    let (success, stdout, stderr) = run(
        "recursive_menu",
        &format!(
            r#"
        {STAGE}
        menu Stream<-F> {{ head: F, tail: Stream<F> }}
        fn stream() -> Stream<(-String -> -i64)> {{
            mu Stream<(-String -> -i64)> {{ head: out <= <deliver | out>, tail: out <= <stream() | out> }}
        }}
        command main | (exit: i32) / {{IO}} {{
            let original = stream();
            let adapted: Stream<(i64 -> String)> = original;
            <10 | adapted.tail.tail.head | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "10\n");
}

#[test]
fn forcing_adapters_survive_multishot_resumption() {
    let (success, stdout, stderr) = run(
        "multishot",
        r#"
        effect Choose { fn choose() -> i64; }
        data Box<-F> { value: F }
        fn make() -> (i64 -> String) / {Choose} {
            let offset = choose(); fn(number: i64) { <(number, offset) | add | int_to_str }
        }
        command main | (exit: i32) / {IO} {
            let original = Box { value: make() };
            let boxed: Box<Delayed<(-String -> -i64), {Choose}>> = original;
            let answer = handle (<1 | boxed.value) {
                choose(): resume => <((<10 | resume), (<20 | resume)) | add
            };
            <answer | println;
            <0 | exit>
        }
    "#,
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "1121\n");
}

#[test]
fn opaque_types_capability_rows_and_unbounded_specialization_are_rejected() {
    for (name, declarations, source, target) in [
        (
            "opaque",
            "data Box<-F> { value: F }",
            "Handler<Box<(-String -> -i64)>, i64, {}, {}>",
            "Handler<Box<(i64 -> String)>, i64, {}, {}>",
        ),
        (
            "capability",
            "effect Reader<-F> { fn read() -> F; } menu Box<E> / {..E} { value: i64 }",
            "Box<{Reader<(-String -> -i64)>}>",
            "Box<{Reader<(i64 -> String)>}>",
        ),
        (
            "nonregular",
            "enum Grow<-F> { End, More(F, Grow<(i64 -> F)>) }",
            "Grow<(-String -> -i64)>",
            "Grow<(i64 -> String)>",
        ),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                r#"
            {declarations}
            fn cast(value: {source}) -> {target} {{ value }}
            command main | (exit: i32) {{ <0 | exit> }}
        "#
            ),
        );
        assert!(!success, "{name} accepted an unsupported adapter");
        assert!(stderr.contains("type:") || stderr.contains("effect:"), "{stderr}");
    }
}

#[test]
fn generic_stage_adapters_use_declared_polarities() {
    let (success, stdout, stderr) = run(
        "generic",
        &format!(
            r#"
        {STAGE}
        data Box<-F> {{ value: F }}
        fn turn<+A, +B>(value: Box<(-B -> -A)>) -> Box<(A -> B)> {{ value }}
        command main | (exit: i32) / {{IO}} {{
            let boxed = <Box {{ value: deliver }} | turn;
            <11 | boxed.value | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "11\n");
}

#[test]
fn bundles_and_named_forms_lift_their_components() {
    let (success, stdout, stderr) = run(
        "forms",
        &format!(
            r#"
        {STAGE}
        form Sink<-F> {{ callback: F }}
        command main | (exit: i32) / {{IO}} {{
            let original = (deliver & deliver);
            let bundle: ((i64 -> String) & (i64 -> String)) = original;
            <12 | bundle.0 | println;
            let answer = mu String {{ out <= {{
                let original = select Sink<(i64 -> String)> {{ Sink {{ callback }} => <13 | callback | out> }};
                let adapted: Sink<(-String -> -i64)> = original;
                <Sink {{ callback: deliver }} | adapted>
            }} }};
            <answer | println;
            <0 | exit>
        }}
    "#
        ),
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "12\n13\n");
}

#[test]
fn lazy_lifting_preserves_demand_effects_and_delayed_results() {
    let (success, stdout, stderr) = run(
        "lazy",
        r#"
        use lazy::Lazy;
        effect Build { fn build() -> i64; }
        effect Use { fn use_value(input: i64) -> i64; }
        command main | (exit: i32) / {IO} {
            let original = mu Lazy<(i64 -> i64 / {Use}), {Build}> {
                force: out <= {
                    let offset = build();
                    <fn(input: i64) { <(input, offset) | add | use_value } | out>
                }
            };
            let adapted: Lazy<(-i64 -> -i64 / {Use}), {Build}> = original;
            <"stored" | println;
            let+ ready = handle { let+ result = adapted.force; result } {
                build(): resume => { <"build" | println; <30 | resume }
            };
            <"ready" | println;
            <handle (<4 | ready) {
                use_value(input): resume => { <"use" | println; <input | resume }
            } | println;
            <0 | exit>
        }
    "#,
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "stored\nbuild\nready\nuse\n34\n");
}
