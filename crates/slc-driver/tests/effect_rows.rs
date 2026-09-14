//! Effect rows, checked as part of types (`docs/design-notes/rows-in-types.md`):
//! the cases the name-following effect pass was held to, in today's syntax,
//! run through the whole pipeline.

use std::process::Command;

fn check(name: &str, source: &str) -> Result<String, String> {
    let path = std::env::temp_dir().join(format!("slc_effect_rows_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    if out.status.success() { Ok(stdout) } else { Err(stderr) }
}

fn refused(name: &str, source: &str, fragments: &[&str]) {
    match check(name, source) {
        Ok(stdout) => panic!("{name}: accepted, printing {stdout:?}"),
        Err(stderr) => {
            for fragment in fragments {
                assert!(stderr.contains(fragment), "{name}: missing {fragment:?} in {stderr}");
            }
        }
    }
}

fn accepted(name: &str, source: &str) -> String {
    check(name, source).unwrap_or_else(|stderr| panic!("{name}: {stderr}"))
}

fn with_main(decls: &[&str], body: &str) -> String {
    let mut source = decls.concat();
    source.push_str("command main | (exit: -i32) / {IO} {\n");
    source.push_str(body);
    source.push_str("\n<0 | exit>\n}\n");
    source
}

const EXN: &str = "effect Exn { fn throw(m: String) -> i64; }\n";
const MAIN: &str = "command main | (exit: -i32) / {IO} { <0 | exit> }\n";
const RISKY: &str = "fn risky(x: i64) -> i64 / {Exn} { <\"boom\" | throw }\n";
const APP: &str = "fn app<E>(f: (i64 -> i64 / {..E}), x: i64) -> i64 / {..E} { <x | f }
fn inc(x: i64) -> i64 { <(x, 1) | add }
";

#[test]
fn an_undeclared_effect_is_rejected() {
    refused(
        "undeclared",
        &[EXN, "fn bad(x: i64) -> i64 { <\"no\" | throw }\n", MAIN].concat(),
        &["`bad` performs `Exn`"],
    );
}

#[test]
fn a_declared_effect_is_accepted_and_propagates() {
    // The check is local to every declaration: `caller` calls `risky`, so it
    // declares what `risky` does.
    accepted(
        "propagates",
        &[EXN, RISKY, "fn caller(x: i64) -> i64 / {Exn} { <x | risky }\n", MAIN].concat(),
    );
    refused(
        "propagates_undeclared",
        &[EXN, RISKY, "fn caller(x: i64) -> i64 { <x | risky }\n", MAIN].concat(),
        &["`caller` performs `Exn`"],
    );
}

#[test]
fn a_row_variable_forwards_an_arguments_row() {
    // A pure argument instantiates `E` to the empty row.
    assert_eq!(
        accepted("forwards_pure", &with_main(&[EXN, RISKY, APP], "<(inc, 1) | app | println;")),
        "2\n"
    );
    // An effectful one flows into the caller, which must answer for it.
    refused(
        "forwards_effect",
        &with_main(&[EXN, RISKY, APP], "<(risky, 1) | app | println;"),
        &["`main` performs `Exn`"],
    );
    // Handled at the call, the row is discharged.
    let handled =
        "let r = handle <(risky, 1) | app { throw(m) => -1, return(n) => n };\n<r | println;";
    assert_eq!(accepted("forwards_handled", &with_main(&[EXN, RISKY, APP], handled)), "-1\n");
}

#[test]
fn a_rowless_arrow_is_a_promise_of_purity() {
    let pure_app = "fn app(f: (i64 -> i64), x: i64) -> i64 { <x | f }\n";
    refused(
        "purity",
        &with_main(&[EXN, RISKY, pure_app], "<(risky, 1) | app | println;"),
        &["takes `f` with a pure arrow", "`risky` performs `Exn`"],
    );
}

#[test]
fn an_undeclared_forwarded_row_is_rejected() {
    refused(
        "forwarded_undeclared",
        &[EXN, "fn app<E>(f: (i64 -> i64 / {..E}), x: i64) -> i64 { <x | f }\n", MAIN].concat(),
        &["performs the row `..E`", "add `..E`"],
    );
}

#[test]
fn forwarding_composes_through_the_call_graph() {
    let twice = "fn twice<F>(g: (i64 -> i64 / {..F}), x: i64) -> i64 / {..F} { <(g, (<(g, x) | app)) | app }\n";
    let body =
        "let r = handle <(risky, 8) | twice { throw(m) => -1, return(n) => n };\n<r | println;";
    assert_eq!(accepted("composes", &with_main(&[EXN, RISKY, APP, twice], body)), "-1\n");
}

#[test]
fn an_operation_passed_as_a_value_carries_its_effect() {
    let app = "fn app<E>(f: (String -> i64 / {..E}), x: String) -> i64 / {..E} { <x | f }\n";
    refused(
        "operation_value",
        &with_main(&[EXN, app], "<(throw, \"m\") | app | println;"),
        &["`main` performs `Exn`"],
    );
}

#[test]
fn a_row_extension_covers_the_named_part() {
    // `{Exn, ..E}` on the parameter: `Exn` is the callee's own business, and
    // only the rest flows through `E`.
    let guard =
        "fn guard<E>(f: (i64 -> i64 / {Exn, ..E}), x: i64) -> i64 / {Exn, ..E} { <x | f }\n";
    let body =
        "let r = handle <(risky, 1) | guard { throw(m) => -1, return(n) => n };\n<r | println;";
    assert_eq!(accepted("extension", &with_main(&[EXN, RISKY, guard], body)), "-1\n");
}

#[test]
fn a_flow_stage_charges_what_it_performs() {
    refused(
        "stage_undeclared",
        &[EXN, "fn risky(n: i64) -> i64 { match (<(n, 0) | gt) { True => n, False => <\"no\" | throw } }\n", MAIN]
            .concat(),
        &["`risky` performs `Exn`"],
    );
    accepted(
        "stage_declared",
        &[
            EXN,
            "fn risky(n: i64) -> i64 / {Exn} { match (<(n, 0) | gt) { True => n, False => <\"no\" | throw } }\n",
            MAIN,
        ]
        .concat(),
    );
}

#[test]
fn printing_performs_io_and_only_main_may_leave_it() {
    refused(
        "shout_undeclared",
        &["fn shout(m: String) -> (,) { <m | println }\n", MAIN].concat(),
        &["`shout` performs `IO`"],
    );
    let shout = "fn shout(m: String) -> (,) / {IO} { <m | println }\n";
    assert_eq!(accepted("shout_declared", &with_main(&[shout], "<\"hi\" | shout;")), "hi\n");
}

#[test]
fn main_may_leave_only_io_undischarged() {
    refused(
        "main_root",
        &[
            EXN,
            "command main | (exit: -i32) / {Exn} { <(<\"no\" | throw) | println; <0 | exit> }\n",
        ]
        .concat(),
        &["`main` is the root"],
    );
}

const FALLIBLE: &str = "menu Fallible / {Exn} { value: i64, doubled: i64 }
fn checked(n: i64) -> Fallible {
    mu Fallible {
        value <= <(match (<(n, 0) | ge) { True => n, False => <\"neg\" | throw }) | value>,
        doubled <= <(match (<(n, 0) | ge) { True => <(n, 2) | mul, False => <\"neg\" | throw }) | doubled>,
    }
}
";

#[test]
fn a_rowed_menu_charges_demands_not_the_constructor() {
    // `checked` declares no row: the arms belong to `Fallible`'s latent row,
    // and the demand is what performs it.
    refused(
        "menu_demand",
        &with_main(&[EXN, FALLIBLE], "<(<1 | checked).value | println;"),
        &["`main` performs `Exn`"],
    );
    // Handled around the demand, `main` is pure.
    let handled = "<handle (<1 | checked).value { throw(m) => -1, return(n) => n } | println;";
    assert_eq!(accepted("menu_handled", &with_main(&[EXN, FALLIBLE], handled)), "1\n");
}

#[test]
fn a_mu_arm_beyond_the_latent_row_is_rejected() {
    let noisy = "effect Log { fn log(m: String) -> (,); }
menu Fallible / {Exn} { value: i64 }
fn noisy() -> Fallible {
    mu Fallible { value: out <= <{ <\"x\" | log; 1 } | out> }
}
";
    refused(
        "latent_arm",
        &[EXN, noisy, MAIN].concat(),
        &["performs `Log`", "`Fallible` does not declare"],
    );
}

#[test]
fn a_rowed_form_charges_the_feed() {
    let guarded = "form Guarded / {Exn} { value: i64 }
fn guard(k: -i64) -> Guarded {
    select Guarded { Guarded { value } => <(<\"no\" | throw) | k> }
}
";
    let body = "let n = mu i64 { k <= <Guarded { value: 1 } | (<k | guard)> };\n<n | println;";
    refused("form_feed", &with_main(&[EXN, guarded], body), &["`main` performs `Exn`"]);
}

#[test]
fn a_latent_result_row_fires_at_the_cut_and_survives_a_handle() {
    // `after` performs nothing when called: its row lives on the consumer it
    // returns. A handler around the call discharges nothing, because nothing
    // ran; around the cut, it discharges the row.
    let after = "fn after<E>(f: (i64 -> i64 / {..E}), k: -i64) -> (-i64 / {..E}) {
    fn(x: i64) { <(<x | f) | k> }
}
";
    let around_call = "let n = mu i64 { out <= {
    let c = handle <(risky, out) | after { throw(m) => select i64 { x => <-1 | out> }, };
    <5 | c>
} };
<n | println;";
    refused(
        "latent_result",
        &with_main(&[EXN, RISKY, after], around_call),
        &["`main` performs `Exn`"],
    );
    let around_cut = "let n = handle (mu i64 { out <= <5 | (<(risky, out) | after)> }) { throw(m) => -1, return(x) => x };
<n | println;";
    assert_eq!(
        accepted("latent_result_handled", &with_main(&[EXN, RISKY, after], around_cut)),
        "-1\n"
    );
}

#[test]
fn a_returned_literal_beyond_the_latent_row_is_rejected() {
    // `/ {}` is the empty row, the same as none, so the literal is charged to
    // `quiet`, which declares nothing.
    refused(
        "returned_literal",
        &[
            EXN,
            "fn quiet(k: -i64) -> (-i64 / {}) {\n    fn(x: i64) { <(<\"loud\" | throw) | k> }\n}\n",
            MAIN,
        ]
        .concat(),
        &["performs `Exn`"],
    );
}

#[test]
fn a_declarations_row_variable_is_one_of_its_row_parameters() {
    refused(
        "undeclared_row",
        &[EXN, "menu Bad / {..E} { value: i64 }\n", MAIN].concat(),
        &["is not one of its row parameters"],
    );
}

#[test]
fn clause_binders_are_copattern_shaped() {
    // Omitted for a clause that never resumes; bound after a colon, under any
    // name, for one that does.
    let never = "let r = handle <\"x\" | throw { throw(m) => -1, return(n) => n };\n<r | println;";
    assert_eq!(accepted("clause_never", &with_main(&[EXN], never)), "-1\n");
    let resumes =
        "let r = handle <\"x\" | throw { throw(m): k => <9 | k, return(n) => n };\n<r | println;";
    assert_eq!(accepted("clause_resumes", &with_main(&[EXN], resumes)), "9\n");
}

#[test]
fn a_negative_function_carries_an_effect_row() {
    let log = "effect Log { fn log(m: String) -> (,); }\n";
    accepted(
        "negative_declared",
        &[log, "fn emit(out: -i64) <- i64 / {Log} { <\"x\" | log; <42 | out> }\n", MAIN].concat(),
    );
    refused(
        "negative_undeclared",
        &[log, "fn emit(out: -i64) <- i64 { <\"x\" | log; <42 | out> }\n", MAIN].concat(),
        &["`emit` performs `Log`"],
    );
}
