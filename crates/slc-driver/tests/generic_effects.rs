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
        hook Reader<+T> { func read() -> T; }
        func get<+T>() -> T / {Reader<T>} { read() }
        proc main | (exit: i32) / {IO} {
            let number = do get() hn { read(): resume => <42 | resume };
            let text = do get() hn { read(): resume => <\"hello\" | resume };
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
        hook Reader<+T> { func read() -> T; }
        func number() -> i64 / {Reader<i64>} { read() }
        func text() -> String / {Reader<String>} { read() }
        proc main | (exit: i32) / {IO} {
            let result = do {
                let inner = do text() hn { read(): resume => <\"inner\" | resume };
                <inner | println;
                number()
            } hn { read(): resume => <42 | resume };
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
        hook Reader<+T> { func read() -> T; }
        func number() -> i64 / {Reader<i64>} { read() }
        proc main | (exit: i32) / {IO} {
            let result = do {
                do number() hn { read(): resume => <\"wrong\" | resume }
            } hn { read(): resume => <42 | resume };
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
            hook Reader<+T> {{ func read() -> T; }}
            func unused() -> i64 / {{{row}}} {{ 0 }}
            proc main | (exit: i32) {{ <0 | exit> }}"
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
        hook Reader<+T> { func read() -> T; }
        proc main | (exit: i32) / {IO} {
            let reader: (i64 hn i64 / {Reader<i64>}) = hn Reader {
                read(): resume => <42 | resume
            };
            <(do read() reader) | println;
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
        hook Pair<+T> { func first() -> T; func second() -> T; }
        proc main | (exit: i32) / {IO} {
            let result = do <(first(), second()) | add hn {
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
        hook Pair<+T> { func first() -> T; func second() -> T; }
        proc main | (exit: i32) / {IO} {
            let result = do first() hn {
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
        hook Factor { func factor() -> i64; }
        proc main | (exit: i32) / {IO} {
            let result = do (do {
                let value = <fn(resume: (i64 -> i64 / {Factor})) {
                    <(<1 | resume, <2 | resume) | add
                } | control::shift;
                <(value, factor()) | mul
            } control::reset) hn { factor(): resume => <10 | resume };
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
        proc main | (exit: i32) / {IO} {
            let result = do {
                let inner = do {
                    let value = <fn(resume: (String -> String)) { <\"inner\" | resume } | control::shift;
                    <(value, \"!\") | add
                } control::reset;
                <inner | println;
                let value = <fn(resume: (i64 -> i64 / {IO})) { <2 | resume } | control::shift;
                <(value, 10) | mul
            } control::reset;
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
            "do { let value = <fn(resume: (i64 -> String)) { <1 | resume } | control::shift; <(value, 1) | add } control::reset",
            "Shift",
        ),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "
            proc main | (exit: i32) / {{IO}} {{
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
        sect provider {
            pub data T { value: String }
            pub hook Reader<+T> { func read() -> T; }
            pub func get<+T>() -> T / {Reader<T>} { read() }
        }
        proc main | (exit: i32) / {IO} {
            let result = do provider::get() hn { provider::read(): resume => <42 | resume };
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
        func make() -> ((i64 -> i64 / {IO}) -> i64 / {IO}) / {IO} {
            <\"build\" | println;
            fn(resume: (i64 -> i64 / {IO})) { <(<1 | resume, <2 | resume) | add }
        }
        proc main | (exit: i32) / {IO} {
            let result = do {
                let value = <make() | control::shift;
                <\"resumed\" | println;
                <(value, 10) | mul
            } control::reset;
            <result | println;
            <(do 42 control::reset) | println;
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
        hook Pair<+T> { func first() -> T; func second() -> T; }
        proc main | (exit: i32) / {IO} {
            let result = do {
                do <(first(), second()) | add hn {
                    first(): resume => <7 | resume,
                    _ => forward
                }
            } hn {
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
        ("duplicate", "hook Reader<+T, +T> { func read() -> T; }", ""),
        ("unsigned", "hook Reader<T> { func read() -> T; }", ""),
        ("unknown", "hook Reader<+T> { func read(value: Missing) -> T; }", ""),
        (
            "local_polarity",
            "hook Reader<+T> { func read() -> T; }",
            "let callback: ((,) -> i64 / {Reader<-i64>}) = fn { 0 };",
        ),
        (
            "capability",
            "hook Reader<+T> { func read() -> T; }",
            "let wrong: (i64 hn i64 / {Reader<String>}) = hn Reader { read(): resume => <42 | resume };",
        ),
    ] {
        let (success, _, stderr) = run(
            name,
            &format!(
                "
            {declarations}
            proc main | (exit: i32) {{ {body} <0 | exit> }}"
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
        hook Reader<+T> { func read() -> T; }
        menu Source<+T> / {Reader<T>} { value: T }
        func make<+T>() -> Source<T> {
            mu Source<T> { value <= <read() | value> }
        }
        proc main | (exit: i32) / {IO} {
            let source: Source<i64> = make();
            <(do source.value hn { read(): resume => <10 | resume }) | println;
            <(do source.value hn { read(): resume => <20 | resume }) | println;
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
        hook Factor { func factor() -> i64; }
        proc main | (exit: i32) / {IO} {
            let result = do {
                let value = <fn(resume: (i64 -> i64 / {Factor})) { <2 | resume } | control::shift;
                <(value, factor()) | mul
            } control::reset;
            <result | println;
            <0 | exit>
        }",
    );
    assert!(!success, "capture erased Factor");
    assert!(stderr.contains("Factor"), "{stderr}");
}

/// The clause adds its payload, so the capability is `Echo<i64>`. Generalizing
/// the handler and instantiating that copy at `String` used to pass the
/// checker and die in `__add`.
#[test]
fn a_stored_handler_cannot_be_instantiated_apart_from_its_clause() {
    let (success, _, stderr) = run(
        "apart",
        "
        hook Echo<+T> { func echo(value: T) -> i64; }
        proc main | (exit: i32) / {IO} {
            let h = hn Echo { echo(value): resume => <(<(value, 1) | add) | resume };
            let s = do (<\"hi\" | echo) h;
            <s | println;
            <0 | exit>
        }",
    );
    assert!(!success, "a String use of an i64 clause was accepted");
    assert!(!stderr.contains("__add"), "the mismatch reached the runtime: {stderr}");
    assert!(
        stderr.contains("Echo") || stderr.contains("i64") || stderr.contains("String"),
        "{stderr}"
    );
}

/// The clause returns its payload unchanged, so one handler value serves
/// every instantiation.
#[test]
fn a_parametric_stored_handler_serves_each_instantiation() {
    let (success, stdout, stderr) = run(
        "parametric",
        "
        hook Echo<+T> { func echo(value: T) -> T; }
        proc main | (exit: i32) / {IO} {
            let h = hn Echo { echo(value): resume => <value | resume };
            let n = do (<1 | echo) h;
            let s = do (<\"hi\" | echo) h;
            <n | println;
            <s | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "1\nhi\n");
}

/// `quiet` handles `throw`, but the caller's `throw` arrived through `E`.
/// The outer handler is the one that answers it.
#[test]
fn a_row_polymorphic_handler_does_not_catch_the_callers_effect() {
    let (success, stdout, stderr) = run(
        "tunnel",
        "
        hook Exc { func throw(message: String) -> i64; }
        func quiet<E>(action: ((,) -> i64 / {..E})) -> i64 / {..E} {
            do <(,) | action hn { throw(message) => 0 }
        }
        proc main | (exit: i32) / {IO} {
            let result = do <fn { <\"boom\" | throw } | quiet hn {
                throw(message) => 7
            };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "7\n");
}

/// The same function's own `throw` is not the caller's. Its handler answers.
#[test]
fn a_polymorphic_function_still_handles_its_own_effect() {
    let (success, stdout, stderr) = run(
        "own",
        "
        hook Exc { func throw(message: String) -> i64; }
        func own<E>(x: i64) -> i64 / {..E} {
            do <\"local\" | throw hn { throw(message) => <(x, 3) | add }
        }
        proc main | (exit: i32) / {IO} {
            let result = do <1 | own hn { throw(message) => 7 };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "4\n");
}

/// Without a row parameter the handler is aware of the closure it calls.
#[test]
fn a_monomorphic_handler_catches_its_argument() {
    let (success, stdout, stderr) = run(
        "mono",
        "
        hook Exc { func throw(message: String) -> i64; }
        func catch_all(action: ((,) -> i64 / {Exc})) -> i64 {
            do <(,) | action hn { throw(message) => 0 }
        }
        proc main | (exit: i32) / {IO} {
            let result = <fn { <\"boom\" | throw } | catch_all;
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "0\n");
}

/// A closure built before the caller's handler still reaches that handler.
#[test]
fn a_closure_built_before_the_handler_still_reaches_it() {
    let (success, stdout, stderr) = run(
        "early",
        "
        hook Exc { func throw(message: String) -> i64; }
        func quiet<E>(action: ((,) -> i64 / {..E})) -> i64 / {..E} {
            do <(,) | action hn { throw(message) => 0 }
        }
        proc main | (exit: i32) / {IO} {
            let action = fn { <\"boom\" | throw };
            let result = do <action | quiet hn { throw(message) => 7 };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "7\n");
}

/// A helper the polymorphic function itself calls is its own code, even
/// though the helper is a declaration born at generation zero.
#[test]
fn a_polymorphic_function_handles_a_helper_it_calls() {
    let (success, stdout, stderr) = run(
        "helper",
        "
        hook Exc { func throw(message: String) -> i64; }
        func boom() -> i64 / {Exc} { <\"x\" | throw }
        func own<E>(x: i64) -> i64 / {..E} {
            do boom() hn { throw(message) => <(x, 3) | add }
        }
        proc main | (exit: i32) / {IO} {
            let result = do <1 | own hn { throw(message) => 7 };
            <result | println;
            <0 | exit>
        }",
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "4\n");
}
