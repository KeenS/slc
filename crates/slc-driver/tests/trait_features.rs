//! Default methods, supertraits, and associated types: the refusals the
//! examples do not show.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String) {
    let path = std::env::temp_dir().join(format!("slc_trait_features_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    (out.status.success(), String::from_utf8(out.stderr).unwrap())
}

#[test]
fn a_child_impl_requires_each_parent() {
    let (ok, stderr) = run(
        "parent",
        "enum Hue { Warm, Cool }
trait Rank: Eq { fn place(self: Self) -> i64; }
impl Rank for Hue { fn place(self: Hue) -> i64 { 0 } }
command main | (exit: i32) / {IO} { <0 | exit> }
",
    );
    assert!(!ok);
    assert!(
        stderr.contains("`impl Rank for Hue` requires `Eq`, which is not implemented"),
        "{stderr}"
    );
}

#[test]
fn an_impl_gives_each_associated_type() {
    let (ok, stderr) = run(
        "missing",
        "enum Countdown { Done }
trait Walk { type Item; fn next(self: Self) -> Item; }
impl Walk for Countdown { fn next(self: Countdown) -> i64 { 0 } }
command main | (exit: i32) / {IO} { <0 | exit> }
",
    );
    assert!(!ok);
    assert!(stderr.contains("does not give `Item`"), "{stderr}");
}

#[test]
fn a_pin_refuses_a_different_item() {
    let (ok, stderr) = run(
        "pin",
        "enum Words { One }
trait Walk { type Item; fn next(self: Self) -> Item; }
impl Walk for Words {
    type Item = String;
    fn next(self: Words) -> String { \"one\" }
}
fn number<+T: Walk<Item = i64>>(x: T) -> i64 { <x | next }
command main | (exit: i32) / {IO} {
    <One | number | println;
    <0 | exit>
}
",
    );
    assert!(!ok);
    assert!(stderr.contains("`number` needs `Walk::Item` to be +i64"), "{stderr}");
    assert!(stderr.contains("Words gives +String"), "{stderr}");
}

#[test]
fn an_unfixed_projection_is_refused() {
    let (ok, stderr) = run(
        "open",
        "trait Walk { type Item; fn next(self: Self) -> Item; }
fn bare<+T>(x: T) -> Walk::Item<T> { x }
command main | (exit: i32) / {IO} { <0 | exit> }
",
    );
    assert!(!ok);
    assert!(stderr.contains("`Walk::Item` of `T` is not fixed"), "{stderr}");
}
