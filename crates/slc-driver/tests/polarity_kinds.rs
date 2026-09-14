//! A type variable takes the polarity of the generic parameters it meets, so
//! one that meets both is refused while it is still unsolved, and one that
//! meets only one has that polarity.

use std::process::Command;

fn run(name: &str, source: &str) -> (String, String, bool) {
    let path = std::env::temp_dir().join(format!("slc_polarity_kinds_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.success(),
    )
}

const SIGNED: &str = "fn keep<+T>(x: T) -> (,) { (,) }\nfn feed<-T>(x: T) -> (,) { (,) }\n";

#[test]
fn an_unsolved_type_meeting_both_polarities_is_refused() {
    // `v`'s type is never solved: it meets `keep`'s `<+T>` and `feed`'s `<-T>`.
    let (stdout, stderr, ok) = run(
        "let_both",
        &format!(
            "{SIGNED}
            command main | (exit: -i32) / {{IO}} {{
                let v = mu {{ k <= <0 | exit> }};
                <v | keep;
                <v | feed;
                <0 | exit>
            }}"
        ),
    );
    assert!(!ok, "stdout: {stdout}");
    assert!(stderr.contains("no type is both positive and negative"), "{stderr}");
}

#[test]
fn a_lambda_parameter_meeting_both_polarities_is_refused() {
    let (stdout, stderr, ok) = run(
        "lambda_both",
        &format!(
            "{SIGNED}
            command main | (exit: -i32) / {{IO}} {{
                let f = fn(x) {{ <x | keep; <x | feed }};
                <0 | exit>
            }}"
        ),
    );
    assert!(!ok, "stdout: {stdout}");
    assert!(stderr.contains("no type is both positive and negative"), "{stderr}");
}

#[test]
fn a_lambda_parameter_takes_the_polarity_of_the_parameter_it_meets() {
    // Nothing solves `x`, but `keep` declares `<+T>`, so it is positive.
    let (stdout, stderr, ok) = run(
        "lambda_one",
        &format!(
            "{SIGNED}
            command main | (exit: -i32) / {{IO}} {{
                let f = fn(x) {{ <x | keep }};
                <\"done\" | println;
                <0 | exit>
            }}"
        ),
    );
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout, "done\n");
}
