//! `A -> B` and `B <- A` are different types. A value written one way is not
//! accepted where the other is declared.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_structural_adapters_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    let _ = std::fs::remove_file(&path);
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn a_consumer_first_function_is_not_a_returning_function() {
    let (success, _, stderr) = run(
        "deliver",
        r#"
        data Box<-F> { value: F }
        func deliver(out: String) <- i64 {
            mu i64 { number => <number | int_to_str | out> }
        }
        proc main | (exit: i32) / {IO} {
            let original = Box { value: deliver };
            let boxed: Box<(i64 -> String)> = original;
            <7 | boxed.value | println;
            <0 | exit>
        }
        "#,
    );
    assert!(!success, "{stderr}");
    assert!(stderr.contains("type:"), "{stderr}");
}

#[test]
fn a_chain_uses_each_stage_in_the_orientation_it_has() {
    let (success, stdout, stderr) = run(
        "both",
        r#"
        func plus_one(out: i64) <- i64 { mu i64 { n => <(n, 1) | __add | out> } }
        func double(n: i64) -> i64 { <(n, 2) | __mul }
        proc main | (exit: i32) / {IO} {
            <mu i64 { out <= <20 | plus_one | double | out> } | println;
            <0 | exit>
        }
        "#,
    );
    assert!(success, "{stderr}");
    assert_eq!(stdout, "42\n");
}
