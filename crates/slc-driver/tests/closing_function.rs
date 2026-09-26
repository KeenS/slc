//! A function closed with `>` is diagnosed as the slip it is: `>` delivers to
//! a consumer, and a function is applied by leaving it off.

use std::process::Command;

fn refused(name: &str, body: &str) -> String {
    let path = std::env::temp_dir().join(format!("slc_closing_function_{name}.sl"));
    let source = format!(
        "hook Reader {{ func config() -> i64; }}
        func inc(n: i64) -> i64 {{ <(n, 1) | add }}

        proc main | (exit: -i32) / {{IO}} {{
            {body}
            <0 | exit>
        }}"
    );
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    assert!(!out.status.success(), "{name}: accepted");
    String::from_utf8(out.stderr).unwrap()
}

#[test]
fn resume_closed_with_a_bracket_is_diagnosed() {
    let stderr = refused(
        "resume_value",
        "let r = do config() { config(): resume => <42 | resume> };\n<r | println;",
    );
    assert!(stderr.contains("`resume` is a function"), "{stderr}");
    assert!(stderr.contains("`<… | resume`"), "{stderr}");

    let stderr = refused(
        "resume_alternative",
        "let s: (i64 | String) = do ::0(1) { config(): resume => <::0(3) | resume> };",
    );
    assert!(stderr.contains("`resume` is a function"), "{stderr}");
    assert!(!stderr.contains("is an alternative of a sum"), "{stderr}");
}

#[test]
fn any_function_closed_with_a_bracket_is_diagnosed() {
    let stderr = refused("function", "let v = mu i64 { k <= <41 | inc> };\n<v | println;");
    assert!(stderr.contains("`inc` is a function"), "{stderr}");
}
