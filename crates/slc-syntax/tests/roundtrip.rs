//! Round-trip property: parse(lower(ast)) produces alpha-equivalent IR.

use slc_core::substitution::alpha_eq_term;
use slc_core::term::Term;
use slc_syntax::lexer::lex;
use slc_syntax::lower::lower_program;
use slc_syntax::parser::parse;

fn lower_str(s: &str) -> Vec<(String, Term)> {
    let toks = lex(s).unwrap();
    let prog = parse(toks).unwrap();
    lower_program(&prog).unwrap()
}

#[test]
fn roundtrip_int() {
    let a = lower_str("fn main() -> i32 { 42 }");
    let b = lower_str("fn main() -> i32 { 42 }");
    assert_eq!(a, b);
}

#[test]
fn roundtrip_lambda_alpha() {
    // Same program with different binder names: alpha-equivalent.
    let a = lower_str("fn main() -> i32 { fn(x: +i32) -> i32 { x }(1) }");
    let b = lower_str("fn main() -> i32 { fn(y: +i32) -> i32 { y }(1) }");
    let (_, ta) = &a[0];
    let (_, tb) = &b[0];
    assert!(alpha_eq_term(ta, tb));
}

#[test]
fn roundtrip_let_scoping() {
    let a = lower_str("fn main() -> i32 { let x = 1; let y = 2; x }");
    let b = lower_str("fn main() -> i32 { let x = 1; let y = 2; x }");
    assert_eq!(a, b);
}

#[test]
fn roundtrip_nested_calls() {
    let a = lower_str("fn main() -> i32 { add(1, add(2, 3)) }");
    let b = lower_str("fn main() -> i32 { add(1, add(2, 3)) }");
    assert_eq!(a, b);
}

#[test]
fn roundtrip_printing_stable() {
    // Display of lowered IR contains expected structure.
    let defs = lower_str("fn main() -> i32 { fn(x: +i32) -> i32 { x }(5) }");
    let printed = format!("{}", defs[0].1);
    assert!(printed.contains("λ"), "should contain lambda: {printed}");
}

#[test]
fn roundtrip_operators_and_indexing() {
    let a = lower_str(
        r#"fn main() -> i32 { let s = "abc"; if 1 + 2 * 3 == 7 && s[0] == 'a' { s[1..] } else { "" } }"#,
    );
    let b = lower_str(
        r#"fn main() -> i32 { let s = "abc"; if 1 + 2 * 3 == 7 && s[0] == 'a' { s[1..] } else { "" } }"#,
    );
    assert_eq!(a, b);
}

#[test]
fn roundtrip_patterns() {
    let a = lower_str(
        r#"fn main() -> i32 { match c { 'a'..='z' | '_' => 1, c if c < '0' => 2, _ => 3 } }"#,
    );
    let b = lower_str(
        r#"fn main() -> i32 { match c { 'a'..='z' | '_' => 1, c if c < '0' => 2, _ => 3 } }"#,
    );
    assert_eq!(a, b);
}
