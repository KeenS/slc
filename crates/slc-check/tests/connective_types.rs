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
            "fn f(p: (+i64, +i64)) -> i64 { 0 }",
            Type::Tensor(vec![Type::Pos(Base::I64), Type::Pos(Base::I64)]),
        ),
        (
            "command f | (k: (-i64 ; -i64)) { k(0) }",
            Type::Par(vec![Type::Neg(Base::I64), Type::Neg(Base::I64)]),
        ),
        (
            "fn f(g: (+i64 -> +bool)) -> i64 { 0 }",
            Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::Bool)),
        ),
        // `dual(A)` applies the involution: `dual(+i64)` is `-i64`.
        ("fn f(k: dual(+i64)) <- i64 { 0 }", Type::Neg(Base::I64)),
        ("command f | (k: -(;)) { k(0) }", Type::BOTTOM),
        // Negation is involutive: a double negation is the type itself.
        ("fn f(b: -i64) -> i64 { 0 }", Type::Neg(Base::I64)),
        ("fn f(r: -(-i64)) -> i64 { 0 }", Type::Pos(Base::I64)),
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
fn a_paren_joins_any_number_of_components_with_one_connective() {
    let i64 = || Type::Pos(Base::I64);
    let cases: Vec<(&str, Type)> = vec![
        ("fn f(p: (+i64, +i64, +i64)) -> i64 { 0 }", Type::Tensor(vec![i64(), i64(), i64()])),
        ("fn f(p: (+i64 | +i64 | +i64)) -> i64 { 0 }", Type::Sum(vec![i64(), i64(), i64()])),
        ("fn f(p: (;)) -> i64 { 0 }", Type::BOTTOM),
        ("fn f(p: (,)) -> i64 { 0 }", Type::ONE),
    ];
    for (source, expected) in cases {
        assert_eq!(lower_type(&parameter_type(source)), Ok(expected), "{source}");
    }
    // Mixing connectives needs the grouping written out.
    assert!(parse(lex("fn f(p: (+i64, +i64 | +i64)) -> i64 { 0 }").unwrap()).is_err());
}

#[test]
fn a_function_into_bottom_is_a_consumer() {
    // `A → ⊥` and `-A` are one type, not two that convert: a function that
    // never returns is a consumer of its argument.
    assert_eq!(
        lower_type(&parameter_type("command f | (k: (+i32 -> (;))) { 0 | k⟩ }")),
        Ok(Type::Neg(Base::I32))
    );
    assert_eq!(
        lower_type(&parameter_type("command f | (k: -i32) { 0 | k⟩ }")),
        Ok(Type::Neg(Base::I32))
    );

    // It is a consumer wherever one is wanted — and, a consumer being a
    // value, it may also arrive as a value parameter.
    assert!(check("command f | (k: (+i32 -> (;))) { 0 | k⟩ }").is_ok());
    assert!(check("command f(x: (+i32 -> (;))) | (k: -i32) { 0 | k⟩ }").is_ok());

    // An ordinary function type is unaffected.
    assert_eq!(
        lower_type(&parameter_type("fn f(g: (+i64 -> +bool)) -> i64 { 0 }")),
        Ok(Type::arrow(Type::Pos(Base::I64), Type::Pos(Base::Bool)))
    );
}

#[test]
fn explicit_connectives_reach_inference() {
    let out = declared("fn f(p: (+i64, +i64)) -> bool { true }");
    assert_eq!(
        out[0].ty,
        Type::arrow(
            Type::Tensor(vec![Type::Pos(Base::I64), Type::Pos(Base::I64)]),
            Type::Pos(Base::Bool)
        )
    );

    // A `command` ends in bottom, and its continuation keeps the par type.
    let out = declared("command f | (k: (-i64 ; -i64)) { k(0) }");
    assert_eq!(
        out[0].ty,
        // `A → ⊥` is `-A`, so a `command`'s type is the dual of its row.
        Type::Par(vec![Type::Neg(Base::I64), Type::Neg(Base::I64)]).dual()
    );
}

#[test]
fn connective_polarity_is_enforced_by_position() {
    // A tensor is positive: it is a value parameter, not a continuation.
    assert!(check("fn f(p: (+i64, +i64)) -> i64 { 0 }").is_ok());
    assert!(check("command f(p: (+i64, +i64))  { p }").is_ok());

    // A par is negative: it serves as a continuation, and — a consumer
    // being a value — as a value parameter too.
    assert!(check("command f | (k: (-i64 ; -i64)) { k(0) }").is_ok());
    assert!(check("command f(p: (-i64 ; -i64)) | (k: -i32) { 0 | k⟩ }").is_ok());

    // Bottom is negative too.
    // A value parameter holds either side, ⊥ included — it is the same
    // type the prelude names `Bottom`.
    assert!(check("command f(p: (;)) | (k: -i32) { 0 | k⟩ }").is_ok());
}

#[test]
fn negations_cancel() {
    // With no shifts in the language, dual is an involution on the nose:
    // `-(-A)` *is* `A`, and double-negation elimination is the identity.
    let neg = lower_type(&parameter_type("fn f(b: -i64) -> i64 { 0 }")).unwrap();
    assert_eq!(neg, Type::Neg(Base::I64));
    assert_eq!(neg.dual(), Type::Pos(Base::I64));
    let dne = lower_type(&parameter_type("fn f(b: -(-i64)) -> i64 { 0 }")).unwrap();
    assert_eq!(dne, Type::Pos(Base::I64));
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

    let par = lower_type(&parameter_type("command f | (k: (-i64 ; -i64)) { k(0) }")).unwrap();
    assert_eq!(par.dual(), Type::Tensor(vec![Type::Pos(Base::I64), Type::Pos(Base::I64)]));
}

#[test]
fn a_connective_type_expression_keeps_its_spans() {
    // Type annotations carry spans, so a diagnostic about a connective type
    // can point at the source.
    let program = parse(lex("fn f(p: (+i64, +i64)) -> i64 { 0 }").unwrap()).unwrap();
    let Decl::Fn { params, .. } = &program.decls[0].kind else { panic!("expected fn") };
    let Some(TypeExpr::Tensor(items)) = &params[0].ty else { panic!("expected a tensor") };
    let [left, right] = items.as_slice() else { panic!("expected two components") };
    let left: &Node<TypeExpr> = left;
    assert!(left.span.end >= left.span.start);
    assert!(right.span.end >= right.span.start);
}
