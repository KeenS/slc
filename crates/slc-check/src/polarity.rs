//! Polarity checking for surface programs.

use crate::declarations::{Declarations, enum_types};
use slc_core::types::Type;
use slc_syntax::ast::{
    Decl, EffectRow, Expr, FunctionPolarity, Node, Param, ParamPolarity, Program, TypeExpr,
};
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
    check_type_param_signs(d, declared, diags);
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
        Decl::Data { fields, .. } => {
            for (_, ty) in fields {
                if let Ok(core_ty) = lower_type(ty)
                    && !is_usable_as_field(&core_ty)
                {
                    diags.push(Diagnostic {
                        message: field_message("record field", &core_ty),
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
        Decl::Form { fields, .. } => {
            for (_, ty) in fields {
                if let Ok(core_ty) = lower_type(ty)
                    && !is_usable_as_field(&core_ty)
                {
                    diags.push(Diagnostic {
                        message: field_message("form field", &core_ty),
                        span: d.span,
                    });
                }
            }
        }
        Decl::Menu { items, .. } => {
            for (_, ty) in items {
                if let Ok(core_ty) = lower_type(ty)
                    && !is_usable_as_field(&core_ty)
                {
                    diags.push(Diagnostic {
                        message: field_message("menu item answer", &core_ty),
                        span: d.span,
                    });
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

/// Every generic type parameter states its polarity, `<+T>` or `<-T>`: a type
/// variable carries none of its own, and what is delayed or run depends on
/// it. A row variable — a parameter written `..E` in the signature — ranges
/// over effects, which have no polarity, so it takes no mark.
fn check_type_param_signs(d: &Node<Decl>, declared: &Declarations, diags: &mut Vec<Diagnostic>) {
    let mut types: Vec<&TypeExpr> = Vec::new();
    let mut rows: Vec<&EffectRow> = Vec::new();
    let (type_params, signs) = match &d.kind {
        Decl::Data { type_params, type_param_signs, fields, .. } => {
            types.extend(fields.iter().map(|(_, ty)| ty));
            (type_params, type_param_signs)
        }
        Decl::Enum { type_params, type_param_signs, variants, .. } => {
            types.extend(variants.iter().flat_map(|(_, fields)| fields));
            (type_params, type_param_signs)
        }
        Decl::Menu { type_params, type_param_signs, effects, items, .. }
        | Decl::Form { type_params, type_param_signs, effects, fields: items, .. } => {
            types.extend(items.iter().map(|(_, ty)| ty));
            rows.push(effects);
            (type_params, type_param_signs)
        }
        Decl::Fn { type_params, type_param_signs, params, return_type, effects, .. } => {
            types.extend(params.iter().filter_map(|p| p.ty.as_ref()));
            types.extend(return_type);
            rows.push(effects);
            (type_params, type_param_signs)
        }
        Decl::Command {
            type_params,
            type_param_signs,
            value_params,
            continuation_params,
            return_type,
            effects,
            ..
        } => {
            types.extend(
                value_params.iter().chain(continuation_params).filter_map(|p| p.ty.as_ref()),
            );
            types.extend(return_type);
            rows.push(effects);
            (type_params, type_param_signs)
        }
        _ => return,
    };
    for ty in types.iter().copied() {
        collect_rows(ty, &mut rows);
    }
    let is_row = |name: &str| rows.iter().any(|row| row.tails.iter().any(|tail| tail == name));
    for param in type_params {
        let signed = signs.iter().any(|(name, _)| name == param);
        if is_row(param) && signed {
            diags.push(Diagnostic {
                message: format!(
                    "`{param}` is a row variable, written `..{param}`, and a row has no \
                     polarity: declare it `<{param}>`, without `+` or `-`"
                ),
                span: d.span,
            });
        } else if !is_row(param) && !signed {
            diags.push(Diagnostic {
                message: format!(
                    "type parameter `{param}` does not state its polarity: declare it \
                     `<+{param}>` for positive types or `<-{param}>` for negative ones"
                ),
                span: d.span,
            });
        }
    }
    // A type written in the signature gives each declaration it applies a
    // type of the polarity that declaration's parameter states.
    for ty in types {
        check_applications(ty, signs, declared, d.span, diags);
    }
}

/// Refuse `List<-i64>` where `List` declares `<+T>`, at any depth.
fn check_applications(
    ty: &TypeExpr,
    own: &[(String, ParamPolarity)],
    declared: &Declarations,
    span: slc_syntax::token::Span,
    diags: &mut Vec<Diagnostic>,
) {
    let recurse = |inner: &TypeExpr, diags: &mut Vec<Diagnostic>| {
        check_applications(inner, own, declared, span, diags)
    };
    match ty {
        TypeExpr::Base(_) => {}
        TypeExpr::Apply(name, args) => {
            for (index, ((param, sign), arg)) in
                declared.param_signs(name).iter().zip(args).enumerate()
            {
                if let Some(sign) = sign
                    && let Some(actual) = written_polarity(&arg.kind, own, declared)
                    && actual != *sign
                {
                    let found = match actual {
                        ParamPolarity::Positive => "positive",
                        ParamPolarity::Negative => "negative",
                    };
                    diags.push(Diagnostic {
                        message: format!(
                            "`{name}` declares `<{}{param}>`, and argument {} of this `{name}<…>` \
                             is a {found} type",
                            sign.mark(),
                            index + 1
                        ),
                        span,
                    });
                }
            }
            for arg in args {
                recurse(&arg.kind, diags);
            }
        }
        TypeExpr::Tensor(items)
        | TypeExpr::Par(items)
        | TypeExpr::With(items)
        | TypeExpr::Sum(items) => {
            for item in items {
                recurse(&item.kind, diags);
            }
        }
        TypeExpr::Positive(inner)
        | TypeExpr::Negative(inner)
        | TypeExpr::Dual(inner)
        | TypeExpr::Effectful(inner, _) => recurse(&inner.kind, diags),
        TypeExpr::Fun(from, to) => {
            recurse(&from.kind, diags);
            recurse(&to.kind, diags);
        }
    }
}

/// The polarity a written type has, when it is known from what is written:
/// a parameter's is the one it declares.
fn written_polarity(
    ty: &TypeExpr,
    own: &[(String, ParamPolarity)],
    declared: &Declarations,
) -> Option<ParamPolarity> {
    match ty {
        TypeExpr::Base(name) => match own.iter().find(|(n, _)| n == name) {
            Some((_, sign)) => Some(*sign),
            None => {
                let resolved = declared.resolve(ty)?;
                if resolved.is_positive() && !resolved.is_negative() {
                    Some(ParamPolarity::Positive)
                } else if resolved.is_negative() && !resolved.is_positive() {
                    Some(ParamPolarity::Negative)
                } else {
                    None
                }
            }
        },
        TypeExpr::Apply(name, _) if declared.is_negative_decl(name) => {
            Some(ParamPolarity::Negative)
        }
        TypeExpr::Apply(..) | TypeExpr::Tensor(_) | TypeExpr::Sum(_) => {
            Some(ParamPolarity::Positive)
        }
        TypeExpr::Positive(inner) => written_polarity(&inner.kind, own, declared),
        TypeExpr::Negative(inner) if inner.kind.is_bottom() => Some(ParamPolarity::Negative),
        TypeExpr::Negative(inner) | TypeExpr::Dual(inner) => {
            written_polarity(&inner.kind, own, declared).map(ParamPolarity::flipped)
        }
        TypeExpr::Par(_) | TypeExpr::With(_) | TypeExpr::Fun(..) | TypeExpr::Effectful(..) => {
            Some(ParamPolarity::Negative)
        }
    }
}

/// The effect rows a written type mentions, at any depth.
fn collect_rows<'a>(ty: &'a TypeExpr, rows: &mut Vec<&'a EffectRow>) {
    match ty {
        TypeExpr::Base(_) => {}
        TypeExpr::Apply(_, args)
        | TypeExpr::Tensor(args)
        | TypeExpr::Par(args)
        | TypeExpr::With(args)
        | TypeExpr::Sum(args) => {
            for arg in args {
                collect_rows(&arg.kind, rows);
            }
        }
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) | TypeExpr::Dual(inner) => {
            collect_rows(&inner.kind, rows)
        }
        TypeExpr::Fun(from, to) => {
            collect_rows(&from.kind, rows);
            collect_rows(&to.kind, rows);
        }
        TypeExpr::Effectful(inner, row) => {
            rows.push(row);
            collect_rows(&inner.kind, rows);
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
    // A continuation parameter must be a consumer: control cannot leave
    // through data. A value parameter may hold either side — a consumer is
    // a value like any other.
    let resolved = declared.resolve(param_type);
    let requires_negative = is_cont || p.is_continuation;
    if requires_negative && let TypeExpr::Positive(_) = param_type {
        diags.push(Diagnostic {
            message: format!(
                "parameter {} has explicitly positive type; expected negative (-) polarity",
                p.describe()
            ),
            span,
        });
        return;
    }
    if let Some(ty) = resolved
        && requires_negative
        && !is_negative_type(&ty)
    {
        diags.push(Diagnostic {
            message: format!(
                "parameter {} has type {ty}; expected negative (-) polarity",
                p.describe()
            ),
            span,
        });
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

/// A field holds a value of either polarity: a consumer is a value like
/// any other.
fn is_usable_as_field(t: &Type) -> bool {
    t.is_positive() || t.is_negative()
}

fn field_message(what: &str, ty: &Type) -> String {
    format!("{what} of type {ty} is not a value type")
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
        Expr::Let { value, body, .. } => {
            check_expr(value, declared, diags);
            if let Some(b) = body {
                check_expr(b, declared, diags);
            }
        }
        Expr::Inject { value, .. } => check_expr(value, declared, diags),
        Expr::Pair(items) | Expr::Par(items) => {
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
        Expr::Project { base: body, .. } => check_expr(body, declared, diags),
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
        // The prelude's `Bool`, which these checks do not load.
        let s = &format!("{s}\nenum Bool {{ False, True }}\n");
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        check_program(&prog)
    }

    #[test]
    fn a_consumer_is_a_value() {
        // A value parameter holds either side: a consumer travels bare, and
        // `-(-i64)` is `+i64` by involution.
        assert!(check("fn f(x: -i64) -> i64 { 0 }").is_ok());
        assert!(check("fn f(x: -(-i64)) -> i64 { x }").is_ok());
        assert!(check("command f(x: -i32) | (k: -i32) { <0 | k> }").is_ok());
        // Control still cannot leave through data.
        let diags = check("command f | (j: +i32 & k: -i32) { <0 | k> }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected negative")), "{diags:?}");
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
        // The consumer value parameter is fine now; the positive
        // continuation parameter is the one real error left.
        let r = check("command bad(x: -i32) | (k: +i32) { k(x) }");
        assert!(r.is_err());
        let diags = r.unwrap_err();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn generic_parameters_state_their_polarity() {
        assert!(check("fn k<+T>(ok: -T) <- T { ok(0) }").is_ok());
        assert!(check("fn k<+T>(value: T) -> T { value }").is_ok());
        assert!(check("fn k<-T>(value: T) -> T { value }").is_ok());
        assert!(check("enum Two<+A, -B> { One(A), Other(B) }").is_ok());

        let diags = check("fn k<T>(value: T) -> T { value }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`T` does not state its polarity")),
            "{diags:?}"
        );
        let diags = check("menu Lazy<T> { force: T }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`<+T>`")), "{diags:?}");
    }

    #[test]
    fn a_row_variable_takes_no_polarity() {
        assert!(check("fn run<+A, E>(g: (+i64 -> +A / {..E})) -> A / {..E} { g(0) }").is_ok());
        let diags =
            check("fn run<+A, +E>(g: (+i64 -> +A / {..E})) -> A / {..E} { g(0) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`E` is a row variable")), "{diags:?}");
    }

    #[test]
    fn a_type_application_gives_each_parameter_its_polarity() {
        const LIST: &str = "enum L<+T> { N, C(T, L<T>) }\n";
        assert!(check(&format!("{LIST}fn f<+U>(xs: L<U>) -> i64 {{ 0 }}")).is_ok());
        assert!(check(&format!("{LIST}fn f(xs: L<(i64, -i64)>) -> i64 {{ 0 }}")).is_ok());
        for refused in [
            "fn f(xs: L<(i64 -> i64)>) -> i64 { 0 }",
            "fn f(xs: L<-i64>) -> i64 { 0 }",
            "fn f<-U>(xs: L<U>) -> i64 { 0 }",
            "fn f(xs: L<L<-i64>>) -> i64 { 0 }",
        ] {
            let diags = check(&format!("{LIST}{refused}")).unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains("`L` declares `<+T>`")),
                "{refused}: {diags:?}"
            );
        }
    }

    #[test]
    fn signed_generic_parameters_still_have_polarity() {
        let r = check("fn bad<+T>(ok: +T) <- T { ok(0) }");
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
