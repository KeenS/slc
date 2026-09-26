//! A handler without a `return` clause has its body's type.

use slc_check::expr::check_program;
use slc_syntax::lexer::lex;
use slc_syntax::parser::parse;
use slc_syntax::traits::elaborate;

fn check(source: &str) -> Result<(), Vec<slc_check::Diagnostic>> {
    // The prelude's `Bool`, which these checks do not load.
    let source = format!("{source}\nenum Bool {{ False, True }}\n");
    let program = parse(lex(&source).unwrap()).unwrap();
    let (program, traits) = elaborate(&program).expect("elaborate");
    check_program(&program, &traits)
}

const READER: &str = "hook Reader { func config() -> i64; }\n";

#[test]
fn handler_clauses_bind_exactly_the_operation_parameters() {
    for (operation, invocation, clause, expected) in [
        (
            "func write(path: String, text: String) -> i64;",
            "<(\"path\", \"text\") | write",
            "write(path): resume => <0 | resume",
            "`write` takes 2 parameters, and this clause binds 1",
        ),
        (
            "func read(path: String) -> i64;",
            "<\"path\" | read",
            "read(path, extra): resume => <0 | resume",
            "`read` takes 1 parameter, and this clause binds 2",
        ),
        (
            "func config() -> i64;",
            "config()",
            "config(unit): resume => <0 | resume",
            "`config` takes 0 parameters, and this clause binds 1",
        ),
    ] {
        let source = format!(
            "hook Test {{ {operation} }} func answer() -> i64 {{ do ({invocation}) {{ {clause} }} }}"
        );
        let diagnostics = check(&source).unwrap_err();
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.message.contains(expected)),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn handler_clause_results_match_the_answer_type() {
    let diagnostics = check(&format!(
        "{READER} func answer() -> i64 {{ do config() {{ config() => \"wrong\" }} }}"
    ))
    .unwrap_err();
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message.contains("`op` clause")),
        "{diagnostics:?}"
    );
}

#[test]
fn resumption_results_match_the_handler_answer_type() {
    let diagnostics = check(&format!(
        "{READER}
        func needs_string(value: String) -> String {{ value }}
        func answer() -> i64 {{
            do config() {{ config(): resume => <(<7 | resume) | needs_string }}
        }}"
    ))
    .unwrap_err();
    assert!(!diagnostics.is_empty());
}

#[test]
fn return_clause_determines_the_resumption_answer_type() {
    assert!(
        check(&format!(
            "{READER} func answer() -> String {{
            do config() {{
                config(): resume => <7 | resume,
                return(value) => \"answer\",
            }}
        }}"
        ))
        .is_ok()
    );
    assert!(
        check(&format!(
            "{READER} func answer() -> String {{
            do config() {{
                config() => 7,
                return(value) => \"answer\",
            }}
        }}"
        ))
        .is_err()
    );
}

#[test]
fn handler_clauses_may_leave_through_a_continuation() {
    assert!(
        check(&format!(
            "{READER} func answer(out: -i64) -> i64 {{
            do config() {{ config() => <7 | out> }}
        }}"
        ))
        .is_ok()
    );
}

#[test]
fn a_handler_without_return_has_its_bodys_type() {
    // The body is an `i64`, so the handler is one.
    assert!(
        check(&format!(
            "{READER} func answer() -> i64 {{ do config() {{ config(): resume => <10 | resume }} }}"
        ))
        .is_ok()
    );
    // And it is not a `String`.
    let diags = check(&format!(
        "{READER} func answer() -> String {{ do config() {{ config(): resume => <10 | resume }} }}"
    ))
    .unwrap_err();
    assert!(diags.iter().any(|d| d.message.contains("the body of `answer`")), "{diags:?}");
}
