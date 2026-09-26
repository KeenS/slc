//! A handler without a `return` clause answers its body's value.

use std::process::Command;

#[test]
fn a_handler_without_return_answers_its_bodys_value() {
    let path = std::env::temp_dir().join("slc_handler_without_return.sl");
    std::fs::write(
        &path,
        r#"hook Reader { func config() -> i64; }

        func scaled(x: i64) -> i64 / {Reader} { <(x, config()) | mul }

        proc main | (exit: -i32) / {IO} {
            // Resumed: the body's value, 70.
            let r = do (<7 | scaled) { config(): resume => <10 | resume };
            <r | println;
            // No operation performed at all: still the body's value.
            let s = do (<(2, 3) | add) { config(): resume => <10 | resume };
            <s | println;
            <0 | exit>
        }"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "stderr: {stderr}");
    assert_eq!(stdout, "70\n5\n");
}
