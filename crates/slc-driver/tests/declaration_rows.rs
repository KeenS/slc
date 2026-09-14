//! A menu or form declaration takes a row parameter, as a function does: its
//! latent row is the row argument of each use, so building a value performs
//! nothing and demanding it performs what that use says.

use std::process::Command;

fn run(name: &str, source: &str) -> (bool, String, String) {
    let path = std::env::temp_dir().join(format!("slc_declaration_rows_{name}.sl"));
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

const TICK: &str = "effect Tick { fn tick() -> (,); }\n";

#[test]
fn a_menus_row_is_its_row_argument() {
    let source = format!(
        "{TICK}
        menu Later<+T, E> / {{..E}} {{ value: T }}

        // Building one performs nothing: `f` runs when `value` is demanded.
        fn later<+T, E>(f: ((,) -> T / {{..E}})) -> Later<T, ..E> {{
            mu Later {{ value <= <(<(,) | f) | value> }}
        }}

        command main | (exit: -i32) / {{IO}} {{
            let l = <(fn {{ let u = tick(); 5 }}) | later;
            <\"built\" | println;
            let v = handle l.value {{ tick(): resume => {{ <\"tick\" | println; <(,) | resume }} }};
            <v | println;
            <0 | exit>
        }}"
    );
    let (ok, stdout, stderr) = run("later", &source);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "built\ntick\n5\n");
}

#[test]
fn a_lazy_sequence_performs_where_it_is_demanded() {
    let body = |demand: &str| {
        format!(
            "{TICK}
            use list::List::*;
            fn noisy(n: i64) -> i64 / {{Tick}} {{ let u = tick(); <(n, 2) | mul }}

            command main | (exit: -i32) / {{IO}} {{
                let s = <(noisy, <Cons(1, Cons(2, Nil)) | seq::of_list) | seq::map;
                {demand}
                <0 | exit>
            }}"
        )
    };
    let (ok, stdout, stderr) = run(
        "seq_handled",
        &body(
            "let l = handle (<s | seq::to_list) { tick(): resume => { <\"tick\" | println; <(,) | resume } };
            <l | fmt | println;",
        ),
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "tick\ntick\n[2, 4]\n");

    let (ok, _, stderr) = run("seq_unhandled", &body("<s | seq::to_list | fmt | println;"));
    assert!(!ok);
    assert!(stderr.contains("performs `Tick`"), "{stderr}");
}

#[test]
fn a_row_variable_a_declaration_does_not_declare_is_refused() {
    let (ok, _, stderr) = run(
        "undeclared",
        &format!(
            "{TICK}menu Bad / {{..E}} {{ value: i64 }}\ncommand main | (exit: -i32) / {{IO}} {{ <0 | exit> }}"
        ),
    );
    assert!(!ok);
    assert!(stderr.contains("`..E` in `Bad`'s row is not one of its row parameters"), "{stderr}");
}
