//! A chain stands as a component — of a tuple, a call's arguments, a
//! bundle's items, a data literal's fields — without parentheses of its own:
//! it ends at the `,` or `)` that closes the component.

use std::process::Command;

fn run(name: &str, body: &str) -> String {
    let path = std::env::temp_dir().join(format!("slc_chain_components_{name}.sl"));
    let source = format!(
        "data Pt {{ x: i64, y: i64 }}
        fn inc(n: i64) -> i64 {{ <(n, 1) | add }}

        command main | (exit: -i32) / {{IO}} {{
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
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(out.status.success(), "{name}: {stderr}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn a_chain_is_a_component_without_parentheses() {
    let bare = run(
        "bare",
        "<(<3 | inc, 1) | add | println;
         <(1, <3 | inc) | add | println;
         <(<3 | inc | inc, <4 | inc) | add | println;
         <(<3 | x => (x, 1) | add, 1) | add | println;",
    );
    let wrapped = run(
        "wrapped",
        "<((<3 | inc), 1) | add | println;
         <(1, (<3 | inc)) | add | println;
         <((<3 | inc | inc), (<4 | inc)) | add | println;
         <((<3 | x => (x, 1) | add), 1) | add | println;",
    );
    assert_eq!(bare, "5\n5\n10\n5\n");
    assert_eq!(bare, wrapped);
}

#[test]
fn a_chain_is_a_field_without_parentheses() {
    let out =
        run("field", "let p = Pt { x: <3 | inc, y: <1 | inc };\n<(p.x, p.y) | add | println;");
    assert_eq!(out, "6\n");
}

#[test]
fn a_tuple_stage_keeps_its_meaning() {
    // `(f, g)` after `|` is one stage, a tuple of functions; the chain does
    // not end at its `,`.
    let path = std::env::temp_dir().join("slc_chain_components_tuple_stage.sl");
    std::fs::write(
        &path,
        "fn inc(n: i64) -> i64 { <(n, 1) | add }
        command main | (exit: -i32) / {IO} { <3 | (inc, inc) | println; <0 | exit> }",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc")).arg("run").arg(&path).output().unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("it has type ((+i64 -> +i64), (+i64 -> +i64))"), "{stderr}");
}
