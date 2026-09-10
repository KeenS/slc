//! Polarity checking for surface programs.

use crate::declarations::{Declarations, enum_types};
use slc_core::types::Type;
use slc_syntax::ast::{Decl, Expr, FunctionPolarity, Node, Param, Program, TypeExpr};
use slc_syntax::lower::LowerError;
use slc_syntax::lower::lower_type;

pub use crate::Diagnostic;

#[derive(Debug)]
pub enum CheckError {
    Lower(LowerError),
    Diag(Vec<Diagnostic>),
}

impl From<LowerError> for CheckError {
    fn from(e: LowerError) -> Self {
        CheckError::Lower(e)
    }
}

/// Check that fn parameters are positive, command value parameters positive,
/// and continuation parameters negative.
pub fn check_program(polarity_p: &Program) -> Result<(), Vec<Diagnostic>> {
    let declared = enum_types(polarity_p);
    let mut diags = Vec::new();
    for d in &polarity_p.decls {
        check_decl(d, &declared, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_decl(d: &Node<Decl>, declared: &Declarations, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { params, polarity, type_params, .. } => {
            let generics: std::collections::HashSet<&str> =
                type_params.iter().map(String::as_str).collect();
            for p in params {
                check_param_polarity(
                    p,
                    *polarity == FunctionPolarity::Negative,
                    &generics,
                    declared,
                    d.span,
                    diags,
                );
            }
        }
        Decl::Command { value_params, continuation_params, .. } => {
            for p in value_params {
                check_param_polarity(p, false, &Default::default(), declared, d.span, diags);
            }
            for p in continuation_params {
                check_param_polarity(p, true, &Default::default(), declared, d.span, diags);
            }
        }
        Decl::Struct { fields, .. } => {
            for (_, ty) in fields {
                if let Ok(core_ty) = lower_type(ty)
                    && !is_usable_as_field(&core_ty)
                {
                    diags.push(Diagnostic {
                        message: field_message("struct field", &core_ty),
                        span: d.span,
                    });
                }
            }
        }
        Decl::Enum { variants, .. } => {
            for (_, fields) in variants {
                for ty in fields {
                    if let Ok(core_ty) = lower_type(ty)
                        && !is_usable_as_field(&core_ty)
                    {
                        diags.push(Diagnostic {
                            message: field_message("variant payload", &core_ty),
                            span: d.span,
                        });
                    }
                }
            }
        }
        // Resolved away before this check runs.
        Decl::Mod { .. }
        | Decl::Use { .. }
        | Decl::Trait { .. }
        | Decl::Impl { .. }
        | Decl::Effect { .. } => {}
        Decl::Const { ty, .. } => {
            if let Ok(core_ty) = lower_type(ty)
                && !is_positive_type(&core_ty)
            {
                diags.push(Diagnostic {
                    message: format!("const type {core_ty} must be positive (+)"),
                    span: d.span,
                });
            }
        }
    }
}

fn check_param_polarity(
    p: &Param,
    is_cont: bool,
    generics: &std::collections::HashSet<&str>,
    declared: &Declarations,
    span: slc_syntax::token::Span,
    diags: &mut Vec<Diagnostic>,
) {
    // Nothing was written, so there is no sign to check: an omitted type
    // takes the polarity of the group it stands in.
    let Some(param_type) = &p.ty else {
        return;
    };
    // A bare generic is polarity-polymorphic: it instantiates at each use.
    // An explicitly signed generic keeps its constraint even though its
    // lowered core representation is an unconstrained variable.
    if let Some(name) = bare_type_name(param_type)
        && generics.contains(name)
    {
        return;
    }
    // A value parameter holds data, and a consumer is data only once boxed:
    // `↓-String`, never `-String`.
    let resolved = declared.resolve(param_type);
    let requires_negative = is_cont || p.is_continuation;
    if requires_negative {
        if let TypeExpr::Positive(_) = param_type {
            diags.push(Diagnostic {
                message: format!(
                    "parameter `{}` has explicitly positive type; expected negative (-) polarity",
                    p.name
                ),
                span,
            });
            return;
        }
    } else if let TypeExpr::Negative(_) = param_type {
        // A double negation is the case worth explaining: it reads as a
        // consumer and is not one.
        let message = match &resolved {
            Some(ty) if is_positive_type(ty) => format!(
                "parameter `{}` is written negative but has type {ty}: dual is an involution",
                p.name
            ),
            Some(ty) => format!(
                "parameter `{}` is a consumer of type {ty}, and a value parameter holds data; \
                 box it as ↓{ty}",
                p.name
            ),
            None => format!(
                "parameter `{}` has explicitly negative type; expected positive (+) polarity",
                p.name
            ),
        };
        diags.push(Diagnostic { message, span });
        return;
    }
    if let Some(ty) = resolved {
        let ok = if requires_negative { is_negative_type(&ty) } else { is_positive_type(&ty) };
        if !ok {
            diags.push(Diagnostic {
                message: format!(
                    "parameter `{}` has type {ty}; expected {} polarity",
                    p.name,
                    if requires_negative { "negative (-)" } else { "positive (+)" }
                ),
                span,
            });
        }
    }
}

fn bare_type_name(ty: &TypeExpr) -> Option<&str> {
    match ty {
        TypeExpr::Base(name) => Some(name.as_str()),
        _ => None,
    }
}

fn is_positive_type(t: &Type) -> bool {
    t.is_positive()
}

fn is_negative_type(t: &Type) -> bool {
    t.is_negative()
}

/// A field holds data. A consumer is not data until it is boxed: `↓-i64`,
/// not `-i64`.
fn is_usable_as_field(t: &Type) -> bool {
    t.is_positive()
}

fn field_message(what: &str, ty: &Type) -> String {
    if ty.is_negative() {
        format!("{what} of type {ty} is a consumer, not data; box it as ↓{ty}")
    } else {
        format!("{what} of type {ty} is not data")
    }
}

/// Check expression polarity: mu binders must be negative. Used on an
/// expression in isolation, so it knows no declarations.
pub fn check_expr_polarity(e: &Node<Expr>) -> Result<(), Vec<Diagnostic>> {
    let mut diags = Vec::new();
    check_expr(e, &Default::default(), &mut diags);
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_expr(e: &Node<Expr>, declared: &Declarations, diags: &mut Vec<Diagnostic>) {
    match &e.kind {
        Expr::Lambda { param_type: Some(ty), .. } => {
            if let Ok(core_ty) = lower_type(ty)
                && !core_ty.is_positive()
                && !core_ty.is_negative()
            {
                diags.push(Diagnostic {
                    message: format!(
                        "fn parameter has type {core_ty}; expected positive (+) polarity"
                    ),
                    span: e.span,
                });
            }
        }
        Expr::Mu { continuation_params, body, .. } => {
            for p in continuation_params {
                check_param_polarity(p, true, &Default::default(), declared, e.span, diags);
            }
            check_expr(body, declared, diags);
        }
        Expr::If { cond, then, otherwise } => {
            check_expr(cond, declared, diags);
            check_expr(then, declared, diags);
            if let Some(o) = otherwise {
                check_expr(o, declared, diags);
            }
        }
        Expr::Let { value, body, .. } => {
            check_expr(value, declared, diags);
            if let Some(b) = body {
                check_expr(b, declared, diags);
            }
        }
        Expr::Pair(items) => {
            for i in items {
                check_expr(i, declared, diags);
            }
        }
        Expr::Call { callee, args } => {
            check_expr(callee, declared, diags);
            for a in args {
                check_expr(a, declared, diags);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            check_expr(lhs, declared, diags);
            check_expr(rhs, declared, diags);
        }
        Expr::UnOp { body, .. } | Expr::Project { base: body, .. } => {
            check_expr(body, declared, diags)
        }
        Expr::Index { value, index } => {
            check_expr(value, declared, diags);
            check_expr(index, declared, diags);
        }
        Expr::Slice { value, start, end } => {
            check_expr(value, declared, diags);
            if let Some(start) = start {
                check_expr(start, declared, diags);
            }
            if let Some(end) = end {
                check_expr(end, declared, diags);
            }
        }
        Expr::Cut { value, consumer } => {
            check_expr(value, declared, diags);
            check_expr(consumer, declared, diags);
        }
        Expr::Select { arms, .. } => {
            for arm in arms {
                check_expr(&arm.command, declared, diags);
            }
        }
        Expr::Block(exprs) => {
            for e in exprs {
                check_expr(e, declared, diags);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_syntax::lexer::lex;
    use slc_syntax::parser::parse;

    fn check(s: &str) -> Result<(), Vec<Diagnostic>> {
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        check_program(&prog)
    }

    #[test]
    fn a_double_negation_is_data_and_says_so() {
        // `-(-i64)` is `+i64`, so it is fine as data and wrong as a
        // consumer — and the diagnostic explains which.
        let diags = check("fn f(x: -(-i64)) -> i64 { 0 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("dual is an involution")), "{diags:?}");

        // A consumer is not data until it is boxed: a bare `-i64` value
        // parameter asks for the box, and the boxed form is accepted.
        let diags = check("fn f(x: -i64) -> i64 { 0 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("box it as ↓-i64")), "{diags:?}");
        assert!(check("fn f(x: ↓-i64) -> i64 { 0 }").is_ok());
        let diags = check("command f(x: -i32) | (k: -i32) { 0 @ k }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("box it as ↓-i32")), "{diags:?}");
    }

    #[test]
    fn fn_positive_params_ok() {
        assert!(check("fn add(x: +i32, y: +i32) -> i32 { x }").is_ok());
    }

    #[test]
    fn fn_continuation_params_ok() {
        assert!(check("fn run(k: -i32) <- i32 { k(1) }").is_ok());
    }

    #[test]
    fn command_mixed_ok() {
        assert!(check("command step(x: +i32) | (k: -i32) { k(x) }").is_ok());
    }

    #[test]
    fn command_wrong_polarity_fails() {
        let r = check("command bad(x: -i32) | (k: +i32) { k(x) }");
        assert!(r.is_err());
        let diags = r.unwrap_err();
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn generic_parameters_are_polarity_polymorphic() {
        let r = check("fn k<T>(ok: -T) <- T { ok(0) }");
        assert!(r.is_ok());

        let r = check("fn k<T>(value: T) -> T { value }");
        assert!(r.is_ok());
    }

    #[test]
    fn signed_generic_parameters_still_have_polarity() {
        let r = check("fn bad<T>(ok: +T) <- T { ok(0) }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("expected negative (-) polarity"));
    }

    #[test]
    fn positive_parameter_in_negative_fn_fails() {
        let r = check("fn bad(k: +i32) <- i32 { k(1) }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("negative"));
    }
}
