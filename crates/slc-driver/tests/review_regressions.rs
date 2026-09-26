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

const TICK: &str = "hook Tick { func tick() -> (,); }
    cite list::List::*;
    func noisy(n: i64) -> i64 / {Tick} { let u = tick(); <(n, 2) | mul }
    func count(s: seq::Seq<i64>) -> i64 { <s | seq::to_list | list::length }
    func count_t(s: seq::Seq<i64, {Tick}>) -> i64 / {Tick} { <s | seq::to_list | list::length }\n";

#[test]
fn a_row_argument_under_an_arrow_is_fitted_the_other_way() {
    // A function over pure sequences does not take a ticking one.
    let (ok, _, stderr) = run(
        "contra_refused",
        &format!(
            "{TICK}
            func run(g: (seq::Seq<i64, {{Tick}}> -> i64), s: seq::Seq<i64, {{Tick}}>) -> i64 {{ <s | g }}
            proc main | (exit: -i32) / {{IO}} {{
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
            func run(g: (seq::Seq<i64> -> i64 / {{Tick}}), s: seq::Seq<i64>) -> i64 / {{Tick}} {{ <s | g }}
            proc main | (exit: -i32) / {{IO}} {{
                let s = <Cons(1, Cons(2, Nil)) | seq::of_list;
                let n = do (<(count_t, s) | run) hn {{ tick(): resume => <(,) | resume }};
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
            func first<+T, E>(s: seq::Seq<T, E>) -> i64 / {{..E}} {{
                of s.next {{ seq::Step::Done => 0, seq::Step::Yield(h, rest) => 1 }}
            }}
            proc main | (exit: -i32) / {{IO}} {{
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
        "hook Exn { func throw(m: String) -> i64; }
        proc two<E> | (ok: (-i64 / {..E}) & program: ((;) / {Exn, ..E})) / {..E} {
            do program hn { throw(m) => <0 | ok> }
        }
        proc main | (exit: -i32) / {IO} {
            <(,) | two | (mu i64 { n => { <n | println; <0 | exit> } } & fn { <\"boom\" | throw; <1 | exit> })>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "0\n");
}

#[test]
fn a_bundles_exits_meet_declared_latent_rows_one_by_one() {
    let (ok, stdout, stderr) = run(
        "two_latent_exits",
        "hook Tick { func tick() -> (,); }
        proc choose | (ok: (-i64 / {Tick, IO}) & err: (-String / {IO})) / {Tick, IO} { <1 | ok> }
        proc main | (exit: -i32) / {IO} {
            do (<(,) | choose | (
                mu i64 { n => { let u = tick(); <n | println; <0 | exit> } }
                & mu String { s => { <s | println; <1 | exit> } }
            )>) hn { tick(): resume => { <\"tick\" | println; <(,) | resume } }
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "tick\n1\n");
}

#[test]
fn consumer_arms_cannot_return_unit() {
    for (name, declarations, body) in [
        ("direct", "", "<1 | mu i64 { value => <value | println }>"),
        ("named", "", "let sink = mu i64 { value => <value | println }; <1 | sink>"),
        (
            "form",
            "form Sink / {IO} { value: i64 }",
            "let sink = mu Sink { Sink { value } => <value | println }; <Sink { value: 1 } | sink>",
        ),
    ] {
        let (ok, _, stderr) = run(
            &format!("returning_consumer_{name}"),
            &format!("{declarations} proc main | (exit: -i32) / {{IO}} {{ {body} }}"),
        );
        assert!(!ok, "{name} accepted a returning consumer");
        assert!(stderr.contains("a `mu` arm is a command"), "{name}: {stderr}");
    }
}

#[test]
fn positive_atoms_cannot_be_cut_into_even_after_inference() {
    for (name, declaration) in [
        (
            "generic",
            "func dne<+T>(refuter: T) -> T { mu { continuation <= <continuation | refuter> } }",
        ),
        (
            "named",
            "func dne(refuter: i64) -> i64 { mu { continuation <= <continuation | refuter> } }",
        ),
        (
            "computed",
            "func dne(refuter: i64) -> i64 { mu { continuation <= <continuation | (<refuter | id)> } }",
        ),
        (
            "alias",
            "func dne(refuter: i64) -> i64 { let alias = refuter; mu { continuation <= <continuation | alias> } }",
        ),
    ] {
        let (ok, _, stderr) = run(
            &format!("positive_consumer_{name}"),
            &format!(
                "{declaration} proc main | (exit: -i32) / {{IO}} {{ <42 | dne | println; <0 | exit> }}"
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
        "func dne<+T>(value: -(-T)) -> T { value }
        func consume<+T>(out: T) <- T { out }
        proc main | (exit: -i32) / {IO} {
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
        "func consume<+T>(out: T) <- T { <0 | out> }
        proc main | (exit: -i32) / {IO} { <0 | exit> }",
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
                "hook Resource {{ func acquire() -> i64; func close(resource: i64) -> (,); }}
                proc main | (exit: -i32) / {{IO}} {{
                    let earlier = mu String {{ message => {{ <message | println; <0 | exit> }} }};
                    do {{
                        let resource = acquire();
                        let exit = mu i32 {{ status => {{ <resource | close; <status | exit> }} }};
                        let complain = mu String {{ message => {{ <message | println; <0 | exit> }} }};
                        {finish}
                    }} hn {{
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
        "proc main | (exit: -i32) / {IO} {
            let __discarded = 7;
            let __tail = mu i64 { value => { <\"wrong tail\" | println; <0 | exit> } };
            let __seq0 = mu i64 { value => { <value | println; <0 | exit> } };
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
        "hook Tick { func tick() -> (,); }
        proc c<E> | (program: ((;) / {..E})) / {..E} { program }
        proc main | (exit: -i32) / {IO} {
            <(,) | c | (fn { <\"ran\" | println; <0 | exit> })>
        }",
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "ran\n");
    // Its row is the command's to perform.
    let (ok, _, stderr) = run(
        "rowed_body_charged",
        "hook Tick { func tick() -> (,); }
        proc c<E> | (program: ((;) / {Tick, ..E})) / {..E} { program }
        proc main | (exit: -i32) / {IO} { <(,) | c | (fn { <0 | exit> })> }",
    );
    assert!(!ok);
    assert!(stderr.contains("performs `Tick`"), "{stderr}");
}

#[test]
fn a_stage_read_the_other_way_round_before_a_command_lowers_that_way() {
    let (ok, stdout, stderr) = run(
        "commuted_row_stage",
        "func twice_of(out: i64) <- i64 { fn(x: i64) { <(x, 2) | mul | out> } }
        proc classify(n: i64) | (found: i64 & missing: String) {
            of (<(n, 0) | gt) { True => <n | found>, False => <\"nothing there\" | missing> }
        }
        proc main | (exit: i32) / {IO} {
            <mu i64 { ok <= <7 | twice_of | classify | (ok & mu String { s => <s | str_len | ok> })> } | println;
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
        "proc pairk(n: i64) | (k: (i64, -i64)) / {IO} {
            <(mu i64 { m => <(m, 1) | add | println }, n) | k>
        }
        proc main | (exit: i32) / {IO} { <0 | exit> }",
    );
    assert!(!ok);
    assert!(!stderr.contains("is a function"), "{stderr}");
    assert!(stderr.contains("this consumer takes"), "{stderr}");
}
