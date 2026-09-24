//! A trait may take type parameters. The call's expected type solves them,
//! and a second impl at different arguments is a different impl.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_trait_params_{name}.sl"));
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

const INTO: &str = "trait Present<+U> { fn present(self: Self) -> U; }\n";

#[test]
fn two_destinations_and_a_forwarded_bound() {
    let (ok, stdout, stderr) = run(
        "destinations",
        &format!(
            "{INTO}
enum Wrap {{ Held(i64) }}
impl Present<i64> for Wrap {{
    fn present(self: Wrap) -> i64 {{ match self {{ Held(n) => n }} }}
}}
impl Present<String> for Wrap {{
    fn present(self: Wrap) -> String {{ match self {{ Held(n) => <n | int_to_str }} }}
}}
fn number(w: Wrap) -> i64 {{ <w | present }}
fn to_text<+T: Present<String>>(x: T) -> String {{ <x | present }}
command main | (exit: i32) / {{IO}} {{
    <Held(7) | number | println;
    <Held(7) | to_text | println;
    <0 | exit>
}}"
        ),
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "7\n7\n");
}

#[test]
fn an_unconstrained_call_is_refused() {
    let (ok, _, stderr) = run(
        "open",
        &format!(
            "{INTO}
impl Present<i64> for i64 {{ fn present(self: i64) -> i64 {{ self }} }}
fn ambiguous(n: i64) -> i64 {{ let x = <n | present; 0 }}
command main | (exit: i32) / {{IO}} {{ <0 | exit> }}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("needs a type for `U`"), "{stderr}");
}

#[test]
fn impls_at_the_same_arguments_overlap() {
    let (ok, _, stderr) = run(
        "overlap",
        &format!(
            "{INTO}
impl Present<i64> for i64 {{ fn present(self: i64) -> i64 {{ self }} }}
impl Present<i64> for i64 {{ fn present(self: i64) -> i64 {{ self }} }}
command main | (exit: i32) / {{IO}} {{ <0 | exit> }}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("overlaps"), "{stderr}");
}

#[test]
fn an_impl_supplies_the_traits_arguments() {
    let (ok, _, stderr) = run(
        "arity",
        &format!(
            "{INTO}
impl Present for i64 {{ fn present(self: i64) -> i64 {{ self }} }}
command main | (exit: i32) / {{IO}} {{ <0 | exit> }}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("takes 1 type argument"), "{stderr}");
}

#[test]
fn an_impl_method_matches_the_substituted_signature() {
    let (ok, _, stderr) = run(
        "signature",
        &format!(
            "{INTO}
impl Present<i64> for i64 {{ fn present(self: i64) -> String {{ \"no\" }} }}
command main | (exit: i32) / {{IO}} {{ <0 | exit> }}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("returns i64") && stderr.contains("returns String"), "{stderr}");
}

#[test]
fn a_value_that_does_not_fit_its_width_is_refused() {
    let (ok, _, stderr) = run(
        "narrow",
        "fn as_i8(n: i64) -> i8 { <n | into }
         command main | (exit: i32) / {IO} { <200 | as_i8 | println; <0 | exit> }",
    );
    assert!(!ok);
    assert!(stderr.contains("200 does not fit in i8"), "{stderr}");
}
