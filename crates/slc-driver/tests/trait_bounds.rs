//! A type parameter may carry several bounds: `<+T: Ord + Display>`.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_trait_bounds_{name}.sl"));
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

const LOUD: &str = "trait Loud { fn shout(self: Self) -> String; }
impl Loud for i64 { fn shout(self: i64) -> String { <(<self | fmt, \"!\") | add } }
";

#[test]
fn a_function_uses_every_trait_its_parameter_is_bound_by() {
    let (ok, stdout, stderr) = run(
        "function",
        &format!(
            "{LOUD}
fn largest<+T: Ord + Loud + Display>(a: T, b: T) -> String {{
    let big = match (<(a, b) | gt) {{ True => a, False => b }};
    <(<big | shout, <big | fmt) | add
}}
command main | (exit: i32) / {{IO}} {{
    <(3, 7) | largest | println;
    <0 | exit>
}}"
        ),
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "7!7\n");
}

#[test]
fn an_impl_is_bound_the_same_way() {
    let (ok, stdout, stderr) = run(
        "impl",
        &format!(
            "{LOUD}
data Pair<+T> {{ left: T, right: T }}
impl<+T: Ord + Loud> Loud for Pair<T> {{
    fn shout(self: Pair<T>) -> String {{
        match (<(self.left, self.right) | gt) {{
            True => <self.left | shout,
            False => <self.right | shout,
        }}
    }}
}}
command main | (exit: i32) / {{IO}} {{
    <Pair {{ left: 3, right: 7 }} | shout | println;
    <0 | exit>
}}"
        ),
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "7!\n");
}

#[test]
fn a_bound_that_is_not_met_is_still_refused_by_name() {
    let (ok, _, stderr) = run(
        "unmet",
        &format!(
            "{LOUD}
fn both<+T: Display + Loud>(x: T) -> String {{ <x | shout }}
command main | (exit: i32) / {{IO}} {{
    <\"text\" | both | println;
    <0 | exit>
}}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("Loud"), "{stderr}");
}

#[test]
fn bounds_are_joined_by_plus_and_not_by_a_second_colon() {
    let (ok, _, stderr) = run(
        "colon",
        "fn f<+T: Ord: Display>(x: T) -> T { x }\ncommand main | (exit: i32) { <0 | exit> }",
    );
    assert!(!ok);
    assert!(stderr.contains("`+`") && stderr.contains("T: Ord + Display"), "{stderr}");
}

#[test]
fn a_type_declaration_still_carries_no_bounds() {
    let (ok, _, stderr) = run(
        "data",
        "data Box<+T: Ord + Display> { inner: T }\ncommand main | (exit: i32) { <0 | exit> }",
    );
    assert!(!ok);
    assert!(stderr.contains("carry no bounds"), "{stderr}");
}
