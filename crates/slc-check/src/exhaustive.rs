//! Match exhaustiveness checking: verify that all enum constructors
//! are covered by the match arms.

use slc_syntax::ast::{Decl, Expr, MatchArm, Node, Pattern, Program};
use slc_syntax::token::Span;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

/// Check all match expressions in a program for exhaustiveness.
pub fn check_exhaustiveness(p: &Program) -> Result<(), Vec<Diagnostic>> {
    // Collect enum declarations: name -> set of variant names.
    let mut enums: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for d in &p.decls {
        if let Decl::Enum { name, variants } = &d.kind {
            enums.insert(name.clone(), variants.iter().map(|(v, _)| v.clone()).collect());
        }
    }

    let mut diags = Vec::new();
    for d in &p.decls {
        check_node_decl(d, &enums, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_node_decl(
    d: &Node<Decl>,
    enums: &std::collections::HashMap<String, Vec<String>>,
    diags: &mut Vec<Diagnostic>,
) {
    match &d.kind {
        Decl::Fn { body, .. } => check_expr(body, enums, diags),
        Decl::Command { body, .. } => check_expr(body, enums, diags),
        Decl::Const { value, .. } => check_expr(value, enums, diags),
        Decl::Struct { .. } | Decl::Enum { .. } => {}
    }
}

fn check_expr(
    e: &Node<Expr>,
    enums: &std::collections::HashMap<String, Vec<String>>,
    diags: &mut Vec<Diagnostic>,
) {
    match &e.kind {
        Expr::Match { scrutinee, arms } => {
            check_match(scrutinee, arms, enums, e.span, diags);
            // Recurse into arm bodies
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    check_expr(guard, enums, diags);
                }
                check_expr(&arm.body, enums, diags);
            }
        }
        Expr::Lambda { body, .. } => check_expr(body, enums, diags),
        Expr::Mu { body, .. } => check_expr(body, enums, diags),
        Expr::Call { callee, args } => {
            check_expr(callee, enums, diags);
            for a in args {
                check_expr(a, enums, diags);
            }
        }
        Expr::Pair(items) => {
            for i in items {
                check_expr(i, enums, diags);
            }
        }
        Expr::Let { value, body, .. } => {
            check_expr(value, enums, diags);
            if let Some(b) = body {
                check_expr(b, enums, diags);
            }
        }
        Expr::If { cond, then, otherwise } => {
            check_expr(cond, enums, diags);
            check_expr(then, enums, diags);
            if let Some(o) = otherwise {
                check_expr(o, enums, diags);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            check_expr(lhs, enums, diags);
            check_expr(rhs, enums, diags);
        }
        Expr::UnOp { body, .. } => check_expr(body, enums, diags),
        Expr::Index { value, index } => {
            check_expr(value, enums, diags);
            check_expr(index, enums, diags);
        }
        Expr::Slice { value, start, end } => {
            check_expr(value, enums, diags);
            if let Some(start) = start {
                check_expr(start, enums, diags);
            }
            if let Some(end) = end {
                check_expr(end, enums, diags);
            }
        }
        Expr::Dual { body } | Expr::Spawn { body } | Expr::ErrorProp { expr: body, .. } => {
            check_expr(body, enums, diags);
        }
        Expr::Interaction { left, right } => {
            check_expr(left, enums, diags);
            check_expr(right, enums, diags);
        }
        Expr::CommandDef { body, .. } => check_expr(body, enums, diags),
        Expr::Service { agent, continuations } => {
            check_expr(agent, enums, diags);
            for k in continuations {
                check_expr(k, enums, diags);
            }
        }
        Expr::Job { agent, values } => {
            check_expr(agent, enums, diags);
            for v in values {
                check_expr(v, enums, diags);
            }
        }
        Expr::Block(exprs) => {
            for ex in exprs {
                check_expr(ex, enums, diags);
            }
        }
        _ => {}
    }
}

fn check_match(
    _scrutinee: &Node<Expr>,
    arms: &[MatchArm],
    enums: &std::collections::HashMap<String, Vec<String>>,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    // An unguarded wildcard always covers everything.
    if arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard) && a.guard.is_none()) {
        return;
    }

    // Collect enum patterns used: Name(variant, _).
    let mut covered: HashSet<String> = HashSet::new();
    let mut scrutinee_type: Option<&String> = None;

    for arm in arms {
        // A binding around a pattern does not change which constructors are
        // covered: `x @ Red` covers exactly what `Red` covers.
        let pattern = match &arm.pattern {
            Pattern::Binding { pattern, .. } => pattern.as_ref(),
            pattern => pattern,
        };
        match pattern {
            Pattern::Ident(x) => {
                // Bare variant name: if it matches a variant of exactly
                // one known enum, treat it as an enum pattern.
                let matching_enums: Vec<&String> =
                    enums.iter().filter(|(_, vs)| vs.contains(x)).map(|(n, _)| n).collect();
                if let Some(enum_name) = matching_enums.first() {
                    if scrutinee_type.is_none() {
                        scrutinee_type = Some(enum_name);
                    }
                    covered.insert(x.clone());
                }
            }
            Pattern::Enum { name, variant, .. } => {
                // Two spellings:
                //   Color::Red  → name=Color, variant=Red
                //   Red         → name=Red, variant=""
                if variant.is_empty() {
                    covered.insert(name.clone());
                } else {
                    if scrutinee_type.is_none() {
                        scrutinee_type = Some(name);
                    }
                    covered.insert(variant.clone());
                }
            }
            _ => {}
        }
    }

    // If we know the enum type, check coverage.
    if let Some(enum_name) = scrutinee_type
        && let Some(variants) = enums.get(enum_name)
    {
        let missing: Vec<&String> = variants.iter().filter(|v| !covered.contains(*v)).collect();
        if !missing.is_empty() {
            diags.push(Diagnostic {
                message: format!(
                    "non-exhaustive match: missing variant{} {} of enum `{enum_name}`",
                    if missing.len() > 1 { "s" } else { "" },
                    missing.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ")
                ),
                span,
            });
        }
        return;
    }

    // Without enum coverage information, a match is exhaustive only when it
    // has an unguarded wildcard. This conservatively rejects guarded
    // wildcards and literal-only matches.
    diags.push(Diagnostic {
        message: "non-exhaustive match: add an unguarded `_` arm".into(),
        span,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_syntax::lexer::lex;
    use slc_syntax::parser::parse;

    fn check(s: &str) -> Result<(), Vec<Diagnostic>> {
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        check_exhaustiveness(&prog)
    }

    #[test]
    fn exhaustive_with_wildcard() {
        assert!(
            check(
                "enum Color { Red, Green, Blue }
             fn f(c: Color) -> i32 { match c { _ => 0 } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn non_exhaustive_detected() {
        let r = check(
            "enum Color { Red, Green, Blue }
             fn f(c: Color) -> i32 { match c { Red => 1, Green => 2 } }",
        );
        assert!(r.is_err());
        let msg = &r.unwrap_err()[0].message;
        assert!(msg.contains("non-exhaustive"));
        assert!(msg.contains("Blue"));
    }

    #[test]
    fn guarded_wildcard_is_not_exhaustive() {
        let r = check("fn f(c: +i64) -> i64 { match c { _ if c > 0 => 1 } }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("non-exhaustive"));
    }

    #[test]
    fn binding_around_enum_pattern_still_counts() {
        assert!(
            check(
                "enum Color { Red, Green, Blue }
             fn f(c: Color) -> i32 { match c { x @ Red => x, Green => 2, Blue => 3 } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn exhaustive_all_variants() {
        assert!(
            check(
                "enum Color { Red, Green, Blue }
             fn f(c: Color) -> i32 { match c { Red => 1, Green => 2, Blue => 3 } }"
            )
            .is_ok()
        );
    }
}
