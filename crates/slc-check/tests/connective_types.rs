//! Every accepted explicit connective type is parsed, lowered, inferred, and
//! polarity-checked consistently.
//!
//! The surface writes the multiplicative connectives, the function arrow, the
//! list former, `dual`, and `⊥` explicitly. Each has to survive the whole
//! front end with the same meaning.

use slc_check::inference::{DeclarationType, infer_program};
use slc_core::types::{Base, Type};
use slc_syntax::ast::{Decl, Node, Param, TypeExpr};
use slc_syntax::lexer::lex;
use slc_syntax::lower::lower_type;
use slc_syntax::parser::parse;

fn parameter_type(source: &str) -> TypeExpr {
    let program = parse(lex(source).unwrap()).unwrap_or_else(|e| panic!("{source}: {e:?}"));
    let decl = &program.decls[0].kind;
    let params: &Vec<Param> = match decl {
        Decl::Fn { params, .. } => params,
        Decl::Command { value_params, continuation_params, .. } => {
            if value_params.is_empty() {
                continuation_params
            } else {
                value_params
            }
        }
        other => panic!("expected a declaration with parameters: {other:?}"),
    };
    params[0].ty.clone().expect("a declaration's parameters carry types")
}

fn declared(source: &str) -> Vec<DeclarationType> {
    let program = parse(lex(source).unwrap()).unwrap_or_else(|e| panic!("{source}: {e:?}"));
    infer_program(&program).unwrap_or_else(|e| panic!("{source}: {e:?}"))
}

fn check(source: &str) -> Result<(), Vec<slc_check::polarity::Diagnostic>> {
    let program = parse(lex(source).unwrap()).unwrap_or_else(|e| panic!("{source}: {e:?}"));
    slc_check::polarity::check_program(&program)
}

#[test]
fn explicit_connectives_parse_and_lower() {
    let cases: Vec<(&str, Type)> = vec![
        (
            "fn f(p: (+i64 ⊗ +i64)) -> i64 { 0 }",
            Type::Tensor(Box::new(Type::Pos(Base::I64)), Box::new(Type::Pos(Base::I64))),
        ),
        (
            "command f | (k: (-i64 ⅋ -i64)) { k(0) }",
            Type::Par(Box::new(Type::Neg(Base::I64)), Box::new(Type::Neg(Base::I64))),
        ),
        (
            "fn f(g: (+i64 -> +bool)) -> i64 { 0 }",
            Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::Bool)),
        ),
        ("fn f(xs: [+i64]) -> i64 { 0 }", Type::List(Box::new(Type::Pos(Base::I64)))),
        // `dual(A)` applies the involution: `dual(+i64)` is `-i64`.
        ("fn f(k: dual(+i64)) <- i64 { 0 }", Type::Neg(Base::I64)),
        ("command f | (k: -⊥) { k(0) }", Type::Bottom),
        // The shifts: `↓` boxes a negative type as data, `↑` is its dual.
        ("fn f(b: ↓-i64) -> i64 { 0 }", Type::Down(Box::new(Type::Neg(Base::I64)))),
        (
            "fn f(r: ↓↑i64) -> i64 { 0 }",
            Type::Down(Box::new(Type::Up(Box::new(Type::Pos(Base::I64))))),
        ),
    ];

    for (source, expected) in cases {
        assert_eq!(
            lower_type(&parameter_type(source)),
            Ok(expected),
            "{source} lowered unexpectedly"
        );
    }
}

#[test]
fn a_function_into_bottom_is_a_consumer() {
    // `A → ⊥` and `-A` are one type, not two that convert: a function that
    // never returns is a consumer of its argument.
    assert_eq!(
        lower_type(&parameter_type("command f | (k: (+i32 -> ⊥)) { 0 @ k }")),
        Ok(Type::Neg(Base::I32))
    );
    assert_eq!(
        lower_type(&parameter_type("command f | (k: -i32) { 0 @ k }")),
        Ok(Type::Neg(Base::I32))
    );

    // So it is a consumer wherever one is wanted, and nowhere else.
    assert!(check("command f | (k: (+i32 -> ⊥)) { 0 @ k }").is_ok());
    let diags = check("command f(x: (+i32 -> ⊥)) | (k: -i32) { 0 @ k }").unwrap_err();
    assert!(
        diags.iter().any(|d| d.message.contains("has type -i32")),
        "a consumer is not a value parameter: {diags:?}"
    );

    // An ordinary function type is unaffected.
    assert_eq!(
        lower_type(&parameter_type("fn f(g: (+i64 -> +bool)) -> i64 { 0 }")),
        Ok(Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::Bool)))
    );
}

#[test]
fn explicit_connectives_reach_inference() {
    let out = declared("fn f(p: (+i64 ⊗ +i64)) -> bool { true }");
    assert_eq!(
        out[0].ty,
        Type::arrow(
            Type::Tensor(Box::new(Type::Pos(Base::I64)), Box::new(Type::Pos(Base::I64))),
            Type::Pos(Base::Bool)
        )
    );

    // A `command` ends in bottom, and its continuation keeps the par type.
    let out = declared("command f | (k: (-i64 ⅋ -i64)) { k(0) }");
    assert_eq!(
        out[0].ty,
        // `A → ⊥` is `-A`, so a `command`'s type is the dual of its row.
        Type::Par(Box::new(Type::Neg(Base::I64)), Box::new(Type::Neg(Base::I64))).dual()
    );
}

#[test]
fn connective_polarity_is_enforced_by_position() {
    // A tensor is positive: it is a value parameter, not a continuation.
    assert!(check("fn f(p: (+i64 ⊗ +i64)) -> i64 { 0 }").is_ok());
    assert!(check("command f(p: (+i64 ⊗ +i64))  { p }").is_ok());

    // A par is negative: it is a continuation, not a value parameter.
    assert!(check("command f | (k: (-i64 ⅋ -i64)) { k(0) }").is_ok());
    let diags = check("command f(p: (-i64 ⅋ -i64))  { p }").unwrap_err();
    assert!(
        diags.iter().any(|d| d.message.contains("expected positive (+) polarity")),
        "a par-typed value parameter should be rejected: {diags:?}"
    );

    // Bottom is negative too.
    let diags = check("command f(p: ⊥)  { p }").unwrap_err();
    assert!(
        diags.iter().any(|d| d.message.contains("expected positive (+) polarity")),
        "a bottom-typed value parameter should be rejected: {diags:?}"
    );
}

#[test]
fn shifts_are_dual_and_never_cancel() {
    // dual(↓B) = ↑dual(B): the involution goes through the box without
    // erasing it, which is what keeps `¬¬A` a different type from `A`.
    let boxed = lower_type(&parameter_type("fn f(b: ↓-i64) -> i64 { 0 }")).unwrap();
    assert_eq!(boxed.dual(), Type::Up(Box::new(Type::Pos(Base::I64))));
    assert_eq!(boxed.dual().dual(), boxed);
    assert!(boxed.is_positive());
    assert!(boxed.dual().is_negative());
    assert_ne!(boxed.dual(), Type::Pos(Base::I64));
}

#[test]
fn duals_of_connectives_are_involutive() {
    // `dual` at the type level is the same involution the core uses, so the
    // surface annotation and the core agree on polarity.
    // `dual(+i64)` and `-i64` are the same type, written two ways.
    let lowered = lower_type(&parameter_type("fn f(k: dual(+i64)) <- i64 { 0 }")).unwrap();
    assert_eq!(lowered, Type::Neg(Base::I64));
    assert_eq!(lowered.dual(), Type::Pos(Base::I64));
    // Only a declaration name stays wrapped, because it is opaque to the core.
    let named = lower_type(&parameter_type("fn f(k: dual(i64)) <- i64 { 0 }")).unwrap();
    assert_eq!(named, Type::Neg(Base::I64));

    let par = lower_type(&parameter_type("command f | (k: (-i64 ⅋ -i64)) { k(0) }")).unwrap();
    assert_eq!(
        par.dual(),
        Type::Tensor(Box::new(Type::Pos(Base::I64)), Box::new(Type::Pos(Base::I64)))
    );
}

#[test]
fn a_connective_type_expression_keeps_its_spans() {
    // Type annotations carry spans, so a diagnostic about a connective type
    // can point at the source.
    let program = parse(lex("fn f(p: (+i64 ⊗ +i64)) -> i64 { 0 }").unwrap()).unwrap();
    let Decl::Fn { params, .. } = &program.decls[0].kind else { panic!("expected fn") };
    let Some(TypeExpr::Tensor(left, right)) = &params[0].ty else { panic!("expected a tensor") };
    let left: &Node<TypeExpr> = left;
    assert!(left.span.end >= left.span.start);
    assert!(right.span.end >= right.span.start);
}
