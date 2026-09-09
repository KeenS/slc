//! Polarity checking for surface programs.

use slc_core::types::Type;
use slc_syntax::ast::{Decl, Expr, Node, Param, Program, TypeExpr};
use slc_syntax::lower::LowerError;
use slc_syntax::lower::lower_type;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: slc_syntax::token::Span,
}

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
    let mut diags = Vec::new();
    for d in &polarity_p.decls {
        check_decl(d, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_decl(d: &Node<Decl>, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { params, .. } => {
            for p in params {
                check_param_polarity(p, p.is_continuation, &d.kind, d.span, diags);
            }
        }
        Decl::Mu { params, .. } => {
            for p in params {
                check_param_polarity(p, p.is_continuation, &d.kind, d.span, diags);
            }
        }
        Decl::Struct { fields, .. } => {
            for (_, ty) in fields {
                if let Ok(core_ty) = lower_type(ty)
                    && !is_usable_as_field(&core_ty)
                {
                    diags.push(Diagnostic {
                        message: format!("struct field type {core_ty} is not a valid field type"),
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
                            message: format!("enum field type {core_ty} is not a valid field type"),
                            span: d.span,
                        });
                    }
                }
            }
        }
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
    current_decl: &Decl,
    span: slc_syntax::token::Span,
    diags: &mut Vec<Diagnostic>,
) {
    if let Ok(ty) = lower_type(&p.ty) {
        let is_fn_decl = matches!(current_decl, Decl::Fn { .. });
        let ok = if is_fn_decl && matches!(p.ty, TypeExpr::Negative(_))
            || is_cont
            || p.is_continuation
        {
            is_negative_type(&ty)
        } else {
            is_positive_type(&ty)
        };
        if !ok {
            diags.push(Diagnostic {
                message: format!(
                    "parameter `{}` has type {ty}; expected {} polarity",
                    p.name,
                    if is_cont { "negative (-)" } else { "positive (+)" }
                ),
                span,
            });
        }
    }
}

fn is_positive_type(t: &Type) -> bool {
    t.is_positive()
}

fn is_negative_type(t: &Type) -> bool {
    t.is_negative()
}

fn is_usable_as_field(t: &Type) -> bool {
    // Fields can be positive (data) or the dual of positive
    t.is_positive() || t.is_negative()
}

/// Check expression polarity: mu binders must be negative.
pub fn check_expr_polarity(e: &Node<Expr>) -> Result<(), Vec<Diagnostic>> {
    let mut diags = Vec::new();
    check_expr(e, &mut diags);
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_expr(e: &Node<Expr>, diags: &mut Vec<Diagnostic>) {
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
        Expr::If { cond, then, otherwise } => {
            check_expr(cond, diags);
            check_expr(then, diags);
            if let Some(o) = otherwise {
                check_expr(o, diags);
            }
        }
        Expr::Let { value, body, .. } => {
            check_expr(value, diags);
            if let Some(b) = body {
                check_expr(b, diags);
            }
        }
        Expr::Pair(items) => {
            for i in items {
                check_expr(i, diags);
            }
        }
        Expr::Call { callee, args } => {
            check_expr(callee, diags);
            for a in args {
                check_expr(a, diags);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            check_expr(lhs, diags);
            check_expr(rhs, diags);
        }
        Expr::UnOp { body, .. } => check_expr(body, diags),
        Expr::Index { value, index } => {
            check_expr(value, diags);
            check_expr(index, diags);
        }
        Expr::Slice { value, start, end } => {
            check_expr(value, diags);
            if let Some(start) = start {
                check_expr(start, diags);
            }
            if let Some(end) = end {
                check_expr(end, diags);
            }
        }
        Expr::Dual { body } => check_expr(body, diags),
        Expr::Interaction { left, right } => {
            check_expr(left, diags);
            check_expr(right, diags);
        }
        Expr::ErrorProp { expr, .. } => check_expr(expr, diags),
        Expr::Service { agent, continuations } => {
            check_expr(agent, diags);
            for k in continuations {
                check_expr(k, diags);
            }
        }
        Expr::Job { agent, values } => {
            check_expr(agent, diags);
            for v in values {
                check_expr(v, diags);
            }
        }
        Expr::Block(exprs) => {
            for e in exprs {
                check_expr(e, diags);
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
    fn fn_positive_params_ok() {
        assert!(check("fn add(x: +i32, y: +i32) -> i32 { x }").is_ok());
    }

    #[test]
    fn fn_continuation_params_ok() {
        assert!(check("fn run(k: -i32) -> i32 { k(1) }").is_ok());
    }

    #[test]
    fn command_mixed_ok() {
        assert!(check("mu step(x: +i32, to k: -i32) { k(x) }").is_ok());
    }

    #[test]
    fn command_wrong_polarity_fails() {
        let r = check("mu bad(x: -i32, to k: +i32) { k(x) }");
        assert!(r.is_err());
        let diags = r.unwrap_err();
        assert_eq!(diags.len(), 2);
    }
}
