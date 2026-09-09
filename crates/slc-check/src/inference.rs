//! Bidirectional inference for surface declarations.
//!
//! This is deliberately conservative: annotations are checked, unannotated
//! ports are represented by core type variables, and the final result must
//! contain no unresolved variables. Generic declarations allocate one core
//! variable per declared type parameter, preserving scoping without yet
//! committing to Rust-style monomorphization.

use slc_core::types::Type;
use slc_core::typing::{TypeError, Unification};
use slc_syntax::ast::{Decl, Expr, Node, Program};
use slc_syntax::lower::{LowerError, lower_type};
use slc_syntax::token::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

#[derive(Debug)]
pub enum InferenceError {
    Lower(LowerError),
    Type(TypeError),
    Diag(Vec<Diagnostic>),
}

impl From<LowerError> for InferenceError {
    fn from(e: LowerError) -> Self {
        Self::Lower(e)
    }
}

impl From<TypeError> for InferenceError {
    fn from(e: TypeError) -> Self {
        Self::Type(e)
    }
}

pub fn infer_program(p: &Program) -> Result<Vec<DeclarationType>, Vec<Diagnostic>> {
    let mut out = Vec::new();
    let mut diags = Vec::new();
    let declared_types: Vec<String> = p
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            Decl::Struct { name, .. } | Decl::Enum { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    for d in &p.decls {
        match infer_decl(d, &declared_types) {
            Ok(ty) => out.push(ty),
            Err(e) => diags.push(Diagnostic { message: format!("inference: {e}"), span: d.span }),
        }
        // Each variant is a declaration of its own: a value of the enum, or a
        // constructor from its payload to the enum.
        if let Decl::Enum { name, variants } = &d.kind {
            for (variant, payload) in variants {
                match variant_type(name, payload) {
                    Ok(ty) => out.push(DeclarationType { name: format!("{name}::{variant}"), ty }),
                    Err(e) => {
                        diags.push(Diagnostic { message: format!("inference: {e}"), span: d.span })
                    }
                }
            }
        }
    }
    if diags.is_empty() { Ok(out) } else { Err(diags) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeclarationType {
    pub name: String,
    pub ty: Type,
}

/// Infer the type of a surface expression using the declaration
/// environment. Expression inference is currently structural and
/// annotation-directed; it handles the final `select` form explicitly.
pub fn infer_expr(
    e: &Node<Expr>,
    declarations: &HashMap<String, Type>,
) -> Result<Type, InferenceError> {
    match &e.kind {
        Expr::Select { ty, .. } => {
            // The consumer of a type is dual to it.
            let consumed = match &ty.kind {
                slc_syntax::ast::TypeExpr::Base(name) => {
                    declarations.get(name).cloned().ok_or_else(|| {
                        InferenceError::Diag(vec![Diagnostic {
                            message: format!("`select {name}` refers to an unknown type"),
                            span: e.span,
                        }])
                    })?
                }
                other => lower_type(other)?,
            };
            Ok(consumed.dual())
        }
        Expr::Int(_) => Ok(Type::Pos(slc_core::types::Base::I32)),
        Expr::Bool(_) => Ok(Type::Pos(slc_core::types::Base::Bool)),
        Expr::Str(_) => Ok(Type::Pos(slc_core::types::Base::Str)),
        Expr::Char(_) => Ok(Type::Pos(slc_core::types::Base::Char)),
        Expr::Ident(name) => declarations.get(name).cloned().ok_or_else(|| {
            InferenceError::Diag(vec![Diagnostic {
                message: format!("inference: unknown declaration `{name}`"),
                span: e.span,
            }])
        }),
        _ => Ok(Type::Var(usize::MAX)),
    }
}

/// The type of an enum variant. A variant with no payload is a value of the
/// declaration; a variant with a payload is a constructor from the packed
/// payload — a right-nested tensor when there is more than one — to it.
pub fn variant_type(
    declaration: &str,
    payload: &[slc_syntax::ast::TypeExpr],
) -> Result<Type, InferenceError> {
    let declared = Type::Named(declaration.to_string());
    let packed = pack(payload)?;
    Ok(match packed {
        None => declared,
        Some(payload) => Type::arrow(payload, declared),
    })
}

/// The tensor representation of a struct declaration: the right-nested
/// product of its field types. A struct with no fields is the tensor unit.
pub fn struct_representation(
    fields: &[(String, slc_syntax::ast::TypeExpr)],
) -> Result<Type, InferenceError> {
    let types: Vec<slc_syntax::ast::TypeExpr> = fields.iter().map(|(_, ty)| ty.clone()).collect();
    Ok(pack(&types)?.unwrap_or(Type::One))
}

/// Pack a list of declared types into one type: nothing, the single type, or
/// a right-nested tensor.
fn pack(types: &[slc_syntax::ast::TypeExpr]) -> Result<Option<Type>, InferenceError> {
    let mut packed: Option<Type> = None;
    for ty in types.iter().rev() {
        let ty = lower_type(ty)?;
        packed = Some(match packed {
            None => ty,
            Some(rest) => Type::Tensor(Box::new(ty), Box::new(rest)),
        });
    }
    Ok(packed)
}

fn infer_decl(
    d: &Node<Decl>,
    declared_types: &[String],
) -> Result<DeclarationType, InferenceError> {
    match &d.kind {
        Decl::Fn { name, type_params, params, return_type, polarity, .. } => {
            let mut u = Unification::new();
            let mut vars: HashMap<&str, Type> = HashMap::new();
            for tp in type_params {
                vars.insert(tp, u.fresh_var());
            }

            let mut inputs = Vec::new();
            for p in params {
                let ty = generic_or_lower(&p.ty, &vars)?;
                let ty =
                    if vars.is_empty() { declaration_or_lower(&p.ty, declared_types)? } else { ty };
                let ty = u.apply(&ty);
                inputs.push(u.unify_with_polarity(&ty, &ty, !p.is_continuation)?);
            }

            let output = match return_type {
                Some(ty) => u.apply(&if vars.is_empty() {
                    declaration_or_lower(ty, declared_types)?
                } else {
                    generic_or_lower(ty, &vars)?
                }),
                None => u.fresh_var(),
            };

            // A negative function produces the *consumer* of the type written
            // after `<-`; its row is unchanged. Dualizing the whole function
            // type would give `A ⊗ -B`, which is a call stack, not a function.
            let output = if *polarity == slc_syntax::ast::FunctionPolarity::Negative {
                output.dual()
            } else {
                output
            };
            let ty = inputs.into_iter().rev().fold(output, Type::arrow_from);
            let ty = if type_params.is_empty() {
                u.resolve_or_cannot_infer(&ty, &format!("fn {name}"))?
            } else {
                ty
            };
            Ok(DeclarationType { name: name.clone(), ty })
        }
        Decl::Mu { name, value_params, continuation_params, .. } => {
            let mut u = Unification::new();
            let mut inputs = Vec::new();
            for p in value_params {
                let ty = lower_type(&p.ty)?;
                inputs.push(u.unify_with_polarity(&ty, &ty, true)?);
            }
            for p in continuation_params {
                let ty = lower_type(&p.ty)?;
                inputs.push(u.unify_with_polarity(&ty, &ty, false)?);
            }
            let output = Type::Bottom;
            let ty = inputs.into_iter().rev().fold(output, Type::arrow_from);
            let ty = u.resolve_or_cannot_infer(&ty, &format!("command {name}"))?;
            Ok(DeclarationType { name: name.clone(), ty })
        }
        Decl::Struct { name, .. } | Decl::Enum { name, .. } => {
            Ok(DeclarationType { name: name.clone(), ty: Type::Named(name.clone()) })
        }
        Decl::Const { name, ty, .. } => {
            Ok(DeclarationType { name: name.clone(), ty: lower_type(ty)? })
        }
    }
}

fn generic_or_lower(
    ty: &slc_syntax::ast::TypeExpr,
    vars: &HashMap<&str, Type>,
) -> Result<Type, InferenceError> {
    if let slc_syntax::ast::TypeExpr::Base(name) = ty
        && let Some(ty) = vars.get(name.as_str())
    {
        return Ok(ty.clone());
    }
    if let slc_syntax::ast::TypeExpr::Positive(inner) = ty
        && let slc_syntax::ast::TypeExpr::Base(name) = &inner.kind
        && let Some(ty) = vars.get(name.as_str())
    {
        return Ok(ty.clone());
    }
    if let slc_syntax::ast::TypeExpr::Negative(inner) = ty
        && let slc_syntax::ast::TypeExpr::Base(name) = &inner.kind
        && let Some(ty) = vars.get(name.as_str())
    {
        return Ok(ty.dual());
    }
    Ok(lower_type(ty)?)
}

fn declaration_or_lower(
    ty: &slc_syntax::ast::TypeExpr,
    declared_types: &[String],
) -> Result<Type, InferenceError> {
    if let slc_syntax::ast::TypeExpr::Base(name) = ty
        && declared_types.iter().any(|declared| declared == name)
    {
        return Ok(Type::Named(name.clone()));
    }
    Ok(lower_type(ty)?)
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceError::Lower(e) => write!(f, "{e}"),
            InferenceError::Type(e) => write!(f, "{e}"),
            InferenceError::Diag(diags) => {
                write!(
                    f,
                    "{}",
                    diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_core::types::Base;
    use slc_syntax::lexer::lex;
    use slc_syntax::parser::parse;

    fn infer(s: &str) -> Result<Vec<DeclarationType>, Vec<Diagnostic>> {
        let p = parse(lex(s).unwrap()).unwrap();
        infer_program(&p)
    }

    #[test]
    fn enum_declaration_and_variant_types_are_precise() {
        let out = infer("enum Shape { Point, Circle(i64), Rect(i64, i64) }").unwrap();
        let ty = |name: &str| {
            out.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("{name}")).ty.clone()
        };
        assert_eq!(ty("Shape"), Type::Named("Shape".into()));
        // A payload-free variant is a value of its declaration.
        assert_eq!(ty("Shape::Point"), Type::Named("Shape".into()));
        // A variant with a payload is a constructor from that payload.
        assert_eq!(
            ty("Shape::Circle"),
            Type::arrow(Type::Pos(Base::I64), Type::Named("Shape".into()))
        );
        // Several payload values are packed into one tensor.
        assert_eq!(
            ty("Shape::Rect"),
            Type::arrow(
                Type::Tensor(Box::new(Type::Pos(Base::I64)), Box::new(Type::Pos(Base::I64))),
                Type::Named("Shape".into())
            )
        );
    }

    #[test]
    fn struct_declaration_lowers_to_a_tensor_of_its_fields() {
        let fields = |source: &str| {
            let p = parse(lex(source).unwrap()).unwrap();
            match &p.decls[0].kind {
                Decl::Struct { fields, .. } => fields.clone(),
                other => panic!("expected a struct: {other:?}"),
            }
        };
        assert_eq!(
            struct_representation(&fields("struct D { left: i64, right: bool }")).unwrap(),
            Type::Tensor(Box::new(Type::Pos(Base::I64)), Box::new(Type::Pos(Base::Bool)))
        );
        assert_eq!(
            struct_representation(&fields("struct One { only: i64 }")).unwrap(),
            Type::Pos(Base::I64)
        );
        // The empty product is the tensor unit.
        assert_eq!(struct_representation(&fields("struct Empty { }")).unwrap(), Type::One);
        // The declaration itself keeps its opaque named type.
        let out = infer("struct D { left: i64, right: bool }").unwrap();
        assert_eq!(out[0].ty, Type::Named("D".into()));
    }

    #[test]
    fn fn_annotation_infers_function_type() {
        let out = infer("fn id(x: +i32) -> i32 { x }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Pos(Base::I32), Type::Pos(Base::I32)));
    }

    #[test]
    fn negative_fn_infers_dual_function_type() {
        // It takes a consumer and produces one: `-i32 → -i32`.
        let out = infer("fn k(x: -i32) <- i32 { x }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Neg(Base::I32), Type::Neg(Base::I32)));
    }

    #[test]
    fn negative_fn_inference_covers_row_arities() {
        let empty = infer("fn k() <- i32 { 0 }").unwrap()[0].ty.clone();
        assert_eq!(empty, Type::Neg(Base::I32));

        let singleton = infer("fn k(ok: -i32) <- i32 { ok(0) }").unwrap()[0].ty.clone();
        assert_eq!(singleton, Type::arrow(Type::Neg(Base::I32), Type::Neg(Base::I32)));

        let multi =
            infer("fn k(ok: -i32, err: -i32) <- i32 { ok(0); err(0) }").unwrap()[0].ty.clone();
        assert_eq!(
            multi,
            Type::arrow(
                Type::Neg(Base::I32),
                Type::arrow(Type::Neg(Base::I32), Type::Neg(Base::I32))
            )
        );
    }

    #[test]
    fn negative_fn_output_is_dual_not_collapsed_by_polarity_unification() {
        let out = infer("fn k(return: -i32) <- i32 { return(0) }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Neg(Base::I32), Type::Neg(Base::I32)));

        // The row and the result stay independent: `-i32 → -bool`.
        let out = infer("fn k(return: -i32) <- bool { return(true) }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Neg(Base::I32), Type::Neg(Base::Bool)));
    }

    #[test]
    fn struct_and_enum_declarations_use_named_types() {
        let out = infer("struct Point { x: i32, y: i32 } enum Color { Red, Green }").unwrap();
        assert_eq!(out[0].ty, Type::Named("Point".into()));
        assert_eq!(out[1].ty, Type::Named("Color".into()));
    }

    #[test]
    fn command_infers_parametric_type() {
        let out = infer("mu step(x: +i32) | (k: -i32) { k(x) }").unwrap();
        assert_eq!(
            out[0].ty,
            Type::arrow(Type::Pos(Base::I32), Type::arrow(Type::Neg(Base::I32), Type::Bottom))
        );
    }

    #[test]
    fn generic_type_variables_are_supported() {
        let out = infer("fn id<T>(x: +T) -> T { x }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Var(0), Type::Var(0)));
    }

    #[test]
    fn generic_negative_functions_preserve_declared_polarity() {
        let out = infer("fn k<T>(ok: -T) <- T { ok(0) }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Var(0).dual(), Type::Var(0)));
    }

    #[test]
    fn generic_function_bare_type_positions_instantiate_to_variables() {
        let out = infer("fn k<T>(value: T) -> T { value }").unwrap();
        assert_eq!(out[0].ty, Type::arrow(Type::Var(0), Type::Var(0)));
    }

    #[test]
    fn select_expression_infers_dual_of_enum() {
        let p = parse(
            lex("enum Color { Red, Green, Blue }
            fn k(return: -i32) <- Color {
                select Color {
                    Red <= return(0),
                    Green <= return(1),
                    Blue <= return(2),
                }
            }")
            .unwrap(),
        )
        .unwrap();
        let decls = infer_program(&p).unwrap();
        let declarations: HashMap<String, Type> =
            decls.into_iter().map(|d| (d.name, d.ty)).collect();
        let e = find_select(&p);
        let ty = infer_expr(&e, &declarations).unwrap();
        assert_eq!(ty, Type::Dual(Box::new(Type::Named("Color".into()))));
    }

    #[test]
    fn select_expression_rejects_unknown_enum() {
        let p = parse(
            lex("fn k(return: -i32) <- i32 {
                select Color {
                    Red <= return(0),
                }
            }")
            .unwrap(),
        )
        .unwrap();
        let declarations = HashMap::new();
        let e = find_select(&p);
        let err = infer_expr(&e, &declarations).unwrap_err();
        assert!(format!("{err}").contains("unknown type"), "{err}");
    }

    fn find_select(p: &Program) -> slc_syntax::ast::Node<slc_syntax::ast::Expr> {
        fn walk(
            e: &slc_syntax::ast::Node<slc_syntax::ast::Expr>,
        ) -> Option<slc_syntax::ast::Node<slc_syntax::ast::Expr>> {
            if matches!(e.kind, Expr::Select { .. }) {
                return Some(slc_syntax::ast::Node { span: e.span, kind: e.kind.clone() });
            }
            match &e.kind {
                Expr::Call { callee, args } => walk(callee).or_else(|| args.iter().find_map(walk)),
                Expr::If { cond, then, otherwise } => walk(cond)
                    .or_else(|| walk(then))
                    .or_else(|| otherwise.as_deref().and_then(walk)),
                Expr::Let { value, body, .. } => {
                    walk(value).or_else(|| body.as_deref().and_then(walk))
                }
                Expr::Block(exprs) => exprs.iter().find_map(walk),
                Expr::Select { .. } => unreachable!(),
                _ => None,
            }
        }
        p.decls
            .iter()
            .find_map(|d| {
                let Decl::Fn { body, .. } = &d.kind else { return None };
                walk(body)
            })
            .expect("select expression")
    }

    #[test]
    fn missing_return_cannot_infer() {
        let p = parse(lex("fn f(x: +i32) { x }").unwrap());
        assert!(p.is_err(), "bare fn unexpectedly parsed: {p:?}");
    }

    #[test]
    fn polarity_constraint_rejects_wrong_polarity() {
        let tokens = lex("fn f(x: -i32) -> i32 { x }").unwrap();
        let p = parse(tokens).unwrap();
        let out = infer_program(&p);
        assert!(out.is_err());
        let message = out.unwrap_err()[0].message.clone();
        assert!(message.contains("positive"), "got: {message}");
    }
}
