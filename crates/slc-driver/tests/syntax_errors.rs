//! A lex or parse error says where it is, as every later phase does, and
//! its guidance names only what exists.

use std::process::Command;

fn stderr_of(name: &str, source: &str) -> String {
    let path = std::env::temp_dir().join(format!("slc_syntax_errors_{name}.sl"));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("failed to run slc");
    assert!(!out.status.success(), "{name} ran");
    String::from_utf8(out.stderr).unwrap()
}

#[test]
fn a_parse_error_names_its_line_and_column_and_quotes_the_token() {
    let stderr = stderr_of(
        "parse",
        "command main | (exit: i32) / {IO} {\n    let x = 1;\n    <x + 2 | exit>\n}\n",
    );
    assert!(stderr.contains("parse error: there is no `+` operator"), "{stderr}");
    assert!(stderr.contains("(at 3:8 `+`)"), "{stderr}");
}

#[test]
fn a_lex_error_names_its_line_and_column() {
    let stderr = stderr_of("lex", "command main | (exit: i32) {\n    <0 # 1 | exit>\n}\n");
    assert!(stderr.contains("unexpected character: #"), "{stderr}");
    assert!(stderr.contains("(at 2:8 `#`)"), "{stderr}");
}

#[test]
fn an_unterminated_string_is_quoted_by_its_first_line_only() {
    let stderr = stderr_of(
        "unterminated",
        "command main | (exit: i32) / {IO} {\n    <\"never closed | println;\n    <0 | exit>\n}\n",
    );
    assert!(stderr.contains("unterminated string"), "{stderr}");
    assert!(stderr.contains("(at 2:6 `\"never closed | println;…`)"), "{stderr}");
    assert!(!stderr.contains("exit>"), "the snippet ran past its line: {stderr}");
}

#[test]
fn a_program_left_open_is_not_blamed_on_the_library() {
    // The library is appended to the program, so a brace or a string left
    // open used to close somewhere in the prelude, and be reported there.
    for (name, source) in [
        ("open_brace", "command main | (exit: i32) / {IO} {\n    <0 | exit>\n"),
        ("open_string", "command main | (exit: i32) / {IO} {\n    <\"open | println;\n}\n"),
    ] {
        let stderr = stderr_of(name, source);
        assert!(!stderr.contains("prelude.sl"), "{name}: {stderr}");
    }
}

#[test]
fn an_error_at_the_end_of_input_claims_no_place_it_does_not_have() {
    let stderr = stderr_of("eof", "fn f() -> i64 {");
    assert!(stderr.contains("parse error:"), "{stderr}");
    assert!(!stderr.contains("(at 1:1"), "{stderr}");
}

#[test]
fn guidance_for_what_is_gone_names_what_exists() {
    // `&&` and `||` are gone. The messages name the `Bool` variants `True`
    // and `False`.
    for (name, body, gone) in [("and", "<a && b", "`&&`"), ("or", "<a || b", "`&&` or `||`")] {
        let stderr = stderr_of(name, &format!("fn f(a: Bool, b: Bool) -> Bool {{ {body} }}"));
        assert!(stderr.contains(gone), "{stderr}");
        assert!(stderr.contains("True =>"), "{stderr}");
        assert!(!stderr.contains("true =>") && !stderr.contains("=> false"), "{stderr}");
    }
}
