//! What a review of the rows, the command row-stage and the handler
//! commands turned up, pinned down.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_review_regressions_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

const TICK: &str = "effect Tick { fn tick() -> (,); }
    use list::List::*;
    fn noisy(n: i64) -> i64 / {Tick} { let u = tick(); <(n, 2) | mul }
    fn count(s: seq::Seq<i64>) -> i64 { <s | seq::to_list | list::length }
    fn count_t(s: seq::Seq<i64, {Tick}>) -> i64 / {Tick} { <s | seq::to_list | list::length }\n";

#[test]
fn a_row_argument_under_an_arrow_is_fitted_the_other_way() {
    // A function over pure sequences does not take a ticking one.
    let (ok, _, stderr) = run(
        "contra_refused",
        &format!(
            "{TICK}
            fn run(g: (seq::Seq<i64, {{Tick}}> -> i64), s: seq::Seq<i64, {{Tick}}>) -> i64 {{ <s | g }}
            command main | (exit: -i32) / {{IO}} {{
                let s = <(noisy, <Cons(1, Nil) | seq::of_list) | seq::map;
                <(<(count, s) | run) | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(!ok, "a pure-sequence function was accepted where a ticking one is required");
    assert!(stderr.contains("performs `Tick`"), "{stderr}");

    // And a function over ticking sequences takes a pure one.
    let (ok, stdout, stderr) = run(
        "contra_accepted",
        &format!(
            "{TICK}
            fn run(g: (seq::Seq<i64> -> i64 / {{Tick}}), s: seq::Seq<i64>) -> i64 / {{Tick}} {{ <s | g }}
            command main | (exit: -i32) / {{IO}} {{
                let s = <Cons(1, Cons(2, Nil)) | seq::of_list;
                let n = handle (<(count_t, s) | run) {{ tick(): resume => <(,) | resume }};
                <n | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "2\n");
}

#[test]
fn a_type_in_a_row_parameters_slot_is_refused() {
    let (ok, _, stderr) = run(
        "bare_row_param",
        &format!(
            "{TICK}
            fn first<+T, E>(s: seq::Seq<T, E>) -> i64 / {{..E}} {{
                match s.next {{ seq::Step::Done => 0, seq::Step::Yield(h, rest) => 1 }}
            }}
            command main | (exit: -i32) / {{IO}} {{
                let s = <(noisy, <Cons(1, Nil) | seq::of_list) | seq::map;
                <(<s | first) | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("declares the row parameter `E` there"), "{stderr}");
    assert!(stderr.contains("write a row, `..E` or `{IO}`"), "{stderr}");
}

#[test]
fn a_command_with_several_exits_takes_the_rowed_ones_row() {
    let (ok, stdout, stderr) = run(
        "two_exits",
        "effect Exn { fn throw(m: String) -> i64; }
        command two<E> | (ok: (-i64 / {..E}) & program: ((;) / {Exn, ..E})) / {..E} {
            handle program { throw(m) => <0 | ok> }
        }
        command main | (exit: -i32) / {IO} {
            <(,) | two | (select i64 { n => { <n | println; <0 | exit> } } & fn { <\"boom\" | throw; <1 | exit> })>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "0\n");
}

#[test]
fn a_bundles_exits_meet_declared_latent_rows_one_by_one() {
    let (ok, stdout, stderr) = run(
        "two_latent_exits",
        "effect Tick { fn tick() -> (,); }
        command choose | (ok: (-i64 / {Tick, IO}) & err: (-String / {IO})) / {Tick, IO} { <1 | ok> }
        command main | (exit: -i32) / {IO} {
            handle (<(,) | choose | (
                select i64 { n => { let u = tick(); <n | println; <0 | exit> } }
                & select String { s => { <s | println; <1 | exit> } }
            )>) { tick(): resume => { <\"tick\" | println; <(,) | resume } }
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "tick\n1\n");
}

#[test]
fn consumer_arms_cannot_return_unit() {
    for (name, declarations, body) in [
        ("direct", "", "<1 | select i64 { value => <value | println }>"),
        ("named", "", "let sink = select i64 { value => <value | println }; <1 | sink>"),
        (
            "form",
            "form Sink / {IO} { value: i64 }",
            "let sink = select Sink { Sink { value } => <value | println }; <Sink { value: 1 } | sink>",
        ),
    ] {
        let (ok, _, stderr) = run(
            &format!("returning_consumer_{name}"),
            &format!("{declarations} command main | (exit: -i32) / {{IO}} {{ {body} }}"),
        );
        assert!(!ok, "{name} accepted a returning consumer");
        assert!(stderr.contains("a `select` arm is a command"), "{name}: {stderr}");
    }
}

#[test]
fn positive_atoms_cannot_be_cut_into_even_after_inference() {
    for (name, declaration) in [
        (
            "generic",
            "fn dne<+T>(refuter: T) -> T { mu { continuation <= <continuation | refuter> } }",
        ),
        (
            "named",
            "fn dne(refuter: i64) -> i64 { mu { continuation <= <continuation | refuter> } }",
        ),
        (
            "computed",
            "fn dne(refuter: i64) -> i64 { mu { continuation <= <continuation | (<refuter | id)> } }",
        ),
        (
            "alias",
            "fn dne(refuter: i64) -> i64 { let alias = refuter; mu { continuation <= <continuation | alias> } }",
        ),
    ] {
        let (ok, _, stderr) = run(
            &format!("positive_consumer_{name}"),
            &format!(
                "{declaration} command main | (exit: -i32) / {{IO}} {{ <42 | dne | println; <0 | exit> }}"
            ),
        );
        assert!(!ok);
        assert!(stderr.contains("the right of a cut must be a consumer"), "{name}: {stderr}");
    }
}

#[test]
fn double_negation_and_generic_consumers_use_identity() {
    let (ok, stdout, stderr) = run(
        "generic_identity_consumers",
        "fn dne<+T>(value: -(-T)) -> T { value }
        fn consume<+T>(out: T) <- T { out }
        command main | (exit: -i32) / {IO} {
            <42 | dne | println;
            <mu i64 { done <= <7 | (<done | consume)> } | println;
            <mu String { done <= <\"text\" | (<done | consume)> } | println;
            <0 | exit>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "42\n7\ntext\n");

    let (ok, _, stderr) = run(
        "generic_consumer_cannot_invent_integer",
        "fn consume<+T>(out: T) <- T { <0 | out> }
        command main | (exit: -i32) / {IO} { <0 | exit> }",
    );
    assert!(!ok);
    assert!(stderr.contains("type:"), "{stderr}");
}

#[test]
fn cleanup_runs_only_through_the_wrapped_exit() {
    for (name, finish, expected) in [
        ("success", "<0 | exit>", "opened\nclosed\n"),
        ("failure", "<\"failed\" | complain>", "opened\nfailed\nclosed\n"),
        ("earlier_capture", "<\"failed\" | earlier>", "opened\nfailed\n"),
    ] {
        let (ok, stdout, stderr) = run(
            &format!("cleanup_{name}"),
            &format!(
                "effect Resource {{ fn acquire() -> i64; fn close(resource: i64) -> (,); }}
                command main | (exit: -i32) / {{IO}} {{
                    let earlier = select String {{ message => {{ <message | println; <0 | exit> }} }};
                    handle {{
                        let resource = acquire();
                        let exit = select i32 {{ status => {{ <resource | close; <status | exit> }} }};
                        let complain = select String {{ message => {{ <message | println; <0 | exit> }} }};
                        {finish}
                    }} {{
                        acquire(): resume => {{ <\"opened\" | println; <1 | resume }},
                        close(resource): resume => {{ <\"closed\" | println; <(,) | resume }}
                    }}
                }}"
            ),
        );
        assert!(ok, "{name}: {stderr}");
        assert_eq!(stdout, expected, "{name}");
    }
}

#[test]
fn block_returns_and_generated_names_are_lexically_scoped() {
    let (ok, stdout, stderr) = run(
        "block_names",
        "command main | (exit: -i32) / {IO} {
            let __discarded = 7;
            let __tail = select i64 { value => { <\"wrong tail\" | println; <0 | exit> } };
            let __seq0 = select i64 { value => { <value | println; <0 | exit> } };
            let answer = { 1; { 2; __discarded } };
            <answer | println;
            let captured = mu i64 { done <= { 3; <8 | done> } };
            <captured | println;
            4;
            <9 | __seq0>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "7\n8\n9\n");
}

#[test]
fn a_rowed_exit_stands_as_a_command_body() {
    let (ok, stdout, stderr) = run(
        "rowed_body",
        "effect Tick { fn tick() -> (,); }
        command c<E> | (program: ((;) / {..E})) / {..E} { program }
        command main | (exit: -i32) / {IO} {
            <(,) | c | (fn { <\"ran\" | println; <0 | exit> })>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "ran\n");
    // Its row is the command's to perform.
    let (ok, _, stderr) = run(
        "rowed_body_charged",
        "effect Tick { fn tick() -> (,); }
        command c<E> | (program: ((;) / {Tick, ..E})) / {..E} { program }
        command main | (exit: -i32) / {IO} { <(,) | c | (fn { <0 | exit> })> }",
    );
    assert!(!ok);
    assert!(stderr.contains("performs `Tick`"), "{stderr}");
}

#[test]
fn a_stage_read_the_other_way_round_before_a_command_lowers_that_way() {
    let (ok, stdout, stderr) = run(
        "commuted_row_stage",
        "fn twice_of(out: i64) <- i64 { fn(x: i64) { <(x, 2) | mul | out> } }
        command classify(n: i64) | (found: i64 & missing: String) {
            match (<(n, 0) | gt) { True => <n | found>, False => <\"nothing there\" | missing> }
        }
        command main | (exit: i32) / {IO} {
            <mu i64 { ok <= <7 | twice_of | classify | (ok & select String { s => <s | str_len | ok> })> } | println;
            <0 | exit>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "14\n");
}

#[test]
fn a_pair_consumer_closed_with_a_bracket_is_not_called_a_function() {
    let (ok, _, stderr) = run(
        "pair_consumer",
        "command pairk(n: i64) | (k: (i64, -i64)) / {IO} {
            <(select i64 { m => <(m, 1) | add | println }, n) | k>
        }
        command main | (exit: i32) / {IO} { <0 | exit> }",
    );
    assert!(!ok);
    assert!(!stderr.contains("is a function"), "{stderr}");
    assert!(stderr.contains("this consumer takes"), "{stderr}");
}
