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

const READER: &str = "effect Reader { fn config() -> i64; }\n";

#[test]
fn a_handler_without_return_has_its_bodys_type() {
    // The body is an `i64`, so the handler is one.
    assert!(
        check(&format!(
            "{READER} fn answer() -> i64 {{ handle config() {{ config(): resume => <10 | resume }} }}"
        ))
        .is_ok()
    );
    // And it is not a `String`.
    let diags = check(&format!(
        "{READER} fn answer() -> String {{ handle config() {{ config(): resume => <10 | resume }} }}"
    ))
    .unwrap_err();
    assert!(diags.iter().any(|d| d.message.contains("the body of `answer`")), "{diags:?}");
}
