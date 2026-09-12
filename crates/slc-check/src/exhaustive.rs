//! Match exhaustiveness checking: verify that all enum constructors
//! are covered by the match arms.

use crate::declarations::{Declarations, enum_types};
use slc_syntax::ast::{Decl, Expr, MatchArm, Named, Node, Pattern, Program};
use slc_syntax::token::Span;
use std::collections::{HashMap, HashSet};

pub use crate::Diagnostic;

/// Check all match expressions in a program for exhaustiveness.
pub fn check_exhaustiveness(p: &Program) -> Result<(), Vec<Diagnostic>> {
    let enums = enum_types(p);
    let mut diags = Vec::new();
    for d in &p.decls {
        check_node_decl(d, &enums, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_node_decl(d: &Node<Decl>, enums: &Declarations, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { params, body, .. } => {
            let bindings = declared_bindings(params);
            check_expr(body, enums, &bindings, diags);
        }
        Decl::Command { value_params, body, .. } => {
            let bindings = declared_bindings(value_params);
            check_expr(body, enums, &bindings, diags);
        }
        Decl::Const { value, .. } => check_expr(value, enums, &HashMap::new(), diags),
        Decl::Data { .. }
        | Decl::Enum { .. }
        | Decl::Menu { .. }
        | Decl::Form { .. }
        | Decl::Mod { .. }
        | Decl::Use { .. }
        | Decl::Trait { .. }
        | Decl::Impl { .. }
        | Decl::Effect { .. } => {}
    }
}

fn declared_bindings(params: &[slc_syntax::ast::Param]) -> HashMap<String, String> {
    params
        .iter()
        .filter(|param| !param.is_continuation)
        .filter_map(|param| {
            written_type_name(param.ty.as_ref()?).map(|ty| (param.name.clone(), ty))
        })
        .collect()
}

fn check_expr(
    e: &Node<Expr>,
    enums: &Declarations,
    bindings: &HashMap<String, String>,
    diags: &mut Vec<Diagnostic>,
) {
    match &e.kind {
        Expr::Match { scrutinee, arms } => {
            check_match(scrutinee, arms, enums, bindings, e.span, diags);
            // Recurse into arm bodies
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    check_expr(guard, enums, bindings, diags);
                }
                check_expr(&arm.body, enums, bindings, diags);
            }
        }
        Expr::Select { ty, arms } => {
            // A `select` covers each shape of its type exactly once: one arm
            // per variant of an `enum`, and exactly one for a product.
            // The written type, or the one an arm names: `Red` is a variant
            // of exactly one enum, and `S { … }` names its struct.
            let written = match ty {
                Some(ty) => written_type_name(&ty.kind),
                None => arms.iter().find_map(|arm| match arm.pattern.names()? {
                    Named::Declaration(name) => Some(name.to_string()),
                    Named::Variant(name) => enums
                        .enums()
                        .find(|(_, variants)| variants.contains(&name.to_string()))
                        .map(|(declaration, _)| declaration.clone()),
                }),
            };
            match written {
                Some(name) if enums.variants_of(&name).is_some() => {
                    check_branch_coverage("select", &name, "variant", arms, enums, e.span, diags);
                }
                // A bare name that is neither declared nor built in is not
                // a type.
                Some(name)
                    if !enums.declares(&name)
                        && ty
                            .as_ref()
                            .is_none_or(|ty| slc_syntax::lower::lower_type(&ty.kind).is_err()) =>
                {
                    diags.push(Diagnostic {
                        message: format!("`select {name}` refers to an unknown type"),
                        span: e.span,
                    });
                }
                // A product has one shape, so it has exactly one arm.
                _ if arms.len() != 1 => {
                    diags.push(Diagnostic {
                        message: format!(
                            "a `select` over a product has exactly one arm; this one has {}",
                            arms.len()
                        ),
                        span: e.span,
                    });
                }
                _ => {}
            }
            for arm in arms {
                check_expr(&arm.command, enums, bindings, diags);
            }
        }
        Expr::CoMatch { ty, arms } => {
            // `mu T { … }` covers each item of its menu exactly once, the
            // way `select` covers each variant of its enum.
            let written = match ty {
                Some(ty) => written_type_name(&ty.kind),
                None => arms.iter().find_map(|arm| {
                    let item = arm_variant(&arm.pattern)?;
                    enums
                        .enums()
                        .find(|(_, items)| items.contains(&item))
                        .map(|(menu, _)| menu.clone())
                }),
            };
            match written {
                Some(name) if enums.variants_of(&name).is_some() => {
                    let rows: Vec<&Pattern> = arms.iter().map(|arm| &arm.pattern).collect();
                    check_comatch_coverage(&name, rows, enums, e.span, diags);
                }
                Some(name) => {
                    diags.push(Diagnostic {
                        message: format!("`mu {name}` refers to an unknown menu"),
                        span: e.span,
                    });
                }
                None => {}
            }
            for arm in arms {
                check_expr(&arm.command, enums, bindings, diags);
            }
        }
        Expr::Lambda { body, .. } => check_expr(body, enums, bindings, diags),
        Expr::Mu { body, .. } => check_expr(body, enums, bindings, diags),
        Expr::Call { callee, args } => {
            check_expr(callee, enums, bindings, diags);
            for a in args {
                check_expr(a, enums, bindings, diags);
            }
        }
        Expr::Pair(items) | Expr::Bundle(items) => {
            for i in items {
                check_expr(i, enums, bindings, diags);
            }
        }
        Expr::Let { value, body, .. } => {
            check_expr(value, enums, bindings, diags);
            if let Some(b) = body {
                check_expr(b, enums, bindings, diags);
            }
        }
        Expr::If { cond, then, otherwise } => {
            check_expr(cond, enums, bindings, diags);
            check_expr(then, enums, bindings, diags);
            if let Some(o) = otherwise {
                check_expr(o, enums, bindings, diags);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            check_expr(lhs, enums, bindings, diags);
            check_expr(rhs, enums, bindings, diags);
        }
        Expr::UnOp { body, .. } | Expr::Project { base: body, .. } => {
            check_expr(body, enums, bindings, diags)
        }
        Expr::Index { value, index } => {
            check_expr(value, enums, bindings, diags);
            check_expr(index, enums, bindings, diags);
        }
        Expr::Slice { value, start, end } => {
            check_expr(value, enums, bindings, diags);
            if let Some(start) = start {
                check_expr(start, enums, bindings, diags);
            }
            if let Some(end) = end {
                check_expr(end, enums, bindings, diags);
            }
        }
        Expr::Handle { body, clauses, ret, .. } => {
            check_expr(body, enums, bindings, diags);
            for c in clauses {
                check_expr(&c.body, enums, bindings, diags);
            }
            if let Some((_, rbody)) = ret {
                check_expr(rbody, enums, bindings, diags);
            }
        }
        Expr::Cut { value, consumer } => {
            check_expr(value, enums, bindings, diags);
            check_expr(consumer, enums, bindings, diags);
        }
        Expr::Block(exprs) => {
            for ex in exprs {
                check_expr(ex, enums, bindings, diags);
            }
        }
        _ => {}
    }
}

/// Does this pattern match every value of its type? A sum needs one arm per
/// variant, but a product has a single shape, so one arm covers it.
fn is_irrefutable(pattern: &Pattern, enums: &Declarations) -> bool {
    match pattern {
        Pattern::Wildcard => true,
        // A name that is not a variant is a binding, so it matches anything.
        Pattern::Ident(name) => enums.payload_arity(name).is_none(),
        Pattern::Binding { pattern, .. } => is_irrefutable(pattern, enums),
        Pattern::Tuple(items) | Pattern::Bundle(items) => {
            items.iter().all(|item| is_irrefutable(item, enums))
        }
        Pattern::Data { name, fields } => {
            enums.declares(name) && fields.iter().all(|(_, pattern)| is_irrefutable(pattern, enums))
        }
        _ => false,
    }
}

/// The name written as a type, if it is a bare declaration name.
fn written_type_name(ty: &slc_syntax::ast::TypeExpr) -> Option<String> {
    match ty {
        slc_syntax::ast::TypeExpr::Base(name) => Some(name.clone()),
        slc_syntax::ast::TypeExpr::Positive(inner) => written_type_name(&inner.kind),
        _ => None,
    }
}

/// The coverage law for a copattern `mu`, recursively: every item of the
/// menu is answered exactly once — by one bound arm, or by a group of
/// nested arms that together cover the item's own menu.
fn check_comatch_coverage(
    menu: &str,
    rows: Vec<&Pattern>,
    enums: &Declarations,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let items = enums.variants_of(menu).cloned().unwrap_or_default();
    let mut order: Vec<&String> = Vec::new();
    let mut groups: std::collections::HashMap<&String, Vec<&Pattern>> =
        std::collections::HashMap::new();
    for pattern in rows {
        let Pattern::Dtor { dtor, arg } = pattern else {
            diags.push(Diagnostic {
                message: format!("`mu {menu}` arm must name an item of `{menu}`"),
                span,
            });
            continue;
        };
        if !groups.contains_key(dtor) {
            order.push(dtor);
        }
        groups.entry(dtor).or_default().push(arg.as_ref());
    }
    let mut seen: HashSet<String> = HashSet::new();
    for dtor in order {
        let group = groups.remove(dtor).expect("grouped above");
        if !items.contains(dtor) {
            diags.push(Diagnostic {
                message: format!("`mu {menu}` refers to unknown item `{dtor}`"),
                span,
            });
            continue;
        }
        seen.insert(dtor.clone());
        if group.iter().all(|arg| matches!(arg, Pattern::Dtor { .. })) {
            // Refined: the group's inner arms must cover the item's menu.
            // (When the item's answer is not a menu, the type checker
            // already said so.)
            if let Some(inner) = enums.nested_menu(&format!("{menu}::{dtor}")) {
                let inner = inner.to_string();
                check_comatch_coverage(&inner, group, enums, span, diags);
            }
        } else if group.len() > 1 {
            diags.push(Diagnostic {
                message: format!("`mu {menu}` has duplicate arm for `{dtor}`"),
                span,
            });
        }
    }
    let missing: Vec<String> = items.iter().filter(|v| !seen.contains(*v)).cloned().collect();
    if !missing.is_empty() {
        diags.push(Diagnostic {
            message: format!("non-exhaustive `mu {menu}`: missing items {}", missing.join(", ")),
            span,
        });
    }
}

/// Every arm names one label of `name`, no label repeats, and none is
/// missing — the coverage law a branch table obeys, shared by `select` over
/// an enum and `mu` over a menu.
fn check_branch_coverage(
    keyword: &str,
    name: &str,
    label_kind: &str,
    arms: &[slc_syntax::ast::SelectArm],
    enums: &Declarations,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let labels = enums.variants_of(name).cloned().unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    for arm in arms {
        let Some(label) = arm_variant(&arm.pattern) else {
            diags.push(Diagnostic {
                message: format!("`{keyword} {name}` arm must name a {label_kind} of `{name}`"),
                span,
            });
            continue;
        };
        if !labels.contains(&label) {
            diags.push(Diagnostic {
                message: format!("`{keyword} {name}` refers to unknown {label_kind} `{label}`"),
                span,
            });
        } else if !seen.insert(label.clone()) {
            diags.push(Diagnostic {
                message: format!("`{keyword} {name}` has duplicate arm for `{label}`"),
                span,
            });
        }
    }
    let missing: Vec<String> = labels.iter().filter(|v| !seen.contains(*v)).cloned().collect();
    if !missing.is_empty() {
        diags.push(Diagnostic {
            message: format!(
                "non-exhaustive `{keyword} {name}`: missing {label_kind}s {}",
                missing.join(", ")
            ),
            span,
        });
    }
}

/// The variant a `select` arm's pattern selects, if it names one.
fn arm_variant(pattern: &Pattern) -> Option<String> {
    match pattern {
        Pattern::Ident(name) => Some(name.clone()),
        Pattern::Enum { name, variant, .. } => {
            Some(if variant.is_empty() { name.clone() } else { variant.clone() })
        }
        Pattern::Dtor { dtor, .. } => Some(dtor.clone()),
        _ => None,
    }
}

/// A variant pattern must bind exactly the payload its variant declares.
fn check_pattern_arity(
    pattern: &Pattern,
    enums: &Declarations,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    match pattern {
        Pattern::Enum { name, variant, fields } => {
            let written =
                if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
            if let Some(arity) = enums.payload_arity(&written)
                && fields.len() != arity
            {
                diags.push(Diagnostic {
                    message: format!(
                        "variant `{written}` carries {arity} payload value(s); the pattern binds {}",
                        fields.len()
                    ),
                    span,
                });
            }
            for field in fields {
                check_pattern_arity(field, enums, span, diags);
            }
        }
        Pattern::Binding { pattern, .. } => check_pattern_arity(pattern, enums, span, diags),
        Pattern::Or(alternatives) => {
            for alternative in alternatives {
                check_pattern_arity(alternative, enums, span, diags);
            }
        }
        Pattern::Tuple(items) | Pattern::Bundle(items) => {
            for item in items {
                check_pattern_arity(item, enums, span, diags);
            }
        }
        Pattern::Data { fields, .. } => {
            for (_, field) in fields {
                check_pattern_arity(field, enums, span, diags);
            }
        }
        _ => {}
    }
}

fn check_match(
    scrutinee: &Node<Expr>,
    arms: &[MatchArm],
    enums: &Declarations,
    bindings: &HashMap<String, String>,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    // An unguarded irrefutable arm covers everything: a wildcard, a plain
    // binding, or the single shape of a product.
    if arms.iter().any(|a| a.guard.is_none() && is_irrefutable(&a.pattern, enums)) {
        return;
    }

    for arm in arms {
        check_pattern_arity(&arm.pattern, enums, span, diags);
    }

    // Collect enum patterns used: Name(variant, _).
    let mut covered: HashSet<String> = HashSet::new();
    let mut scrutinee_type = match &scrutinee.kind {
        Expr::Ident(name) => bindings.get(name),
        _ => None,
    };

    for arm in arms {
        // A binding around a pattern does not change which constructors are
        // covered: `x @ Red` covers exactly what `Red` covers.
        let pattern = match &arm.pattern {
            Pattern::Binding { pattern, .. } => pattern.as_ref(),
            pattern => pattern,
        };
        // Three spellings reach here, and a bare variant name — with or
        // without a payload — names its enum only indirectly:
        //   Color::Red  → name=Color, variant=Red
        //   Red         → name=Red, variant=""
        //   Red         → Pattern::Ident, when it binds no payload
        let (written, payload) = match pattern {
            Pattern::Ident(x) => (x, None),
            Pattern::Enum { name, variant, .. } if variant.is_empty() => (name, None),
            Pattern::Enum { name, variant, .. } => (variant, Some(name)),
            // A request shape covers its item, resolved like an unqualified
            // variant against the menu table.
            Pattern::Dtor { dtor, .. } => (dtor, None),
            _ => continue,
        };
        match payload {
            Some(enum_name) => {
                if scrutinee_type.is_none() {
                    scrutinee_type = Some(enum_name);
                }
                covered.insert(written.clone());
            }
            // Unqualified: if this name is a variant of exactly one known
            // enum, that enum is what the match is over.
            None => {
                let declaring: Option<&String> =
                    enums.enums().find(|(_, vs)| vs.contains(written)).map(|(n, _)| n);
                if let Some(enum_name) = declaring {
                    if scrutinee_type.is_none() {
                        scrutinee_type = Some(enum_name);
                    }
                    covered.insert(written.clone());
                }
            }
        }
    }

    // If we know the enum type, check coverage.
    if let Some(enum_name) = scrutinee_type
        && let Some(variants) = enums.variants_of(enum_name)
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
    fn an_unqualified_variant_pattern_names_its_enum_even_with_a_payload() {
        // `Circle(r)` identifies `Shape` exactly as bare `Red` identifies
        // `Color`; a payload does not make the spelling ambiguous.
        assert!(
            check(
                "enum Shape { Circle(i64), Square(i64) }
             fn area(s: Shape) -> i64 { match s { Circle(r) => r, Square(w) => w } }"
            )
            .is_ok()
        );

        let diags = check(
            "enum Shape { Circle(i64), Square(i64), Dot }
             fn area(s: Shape) -> i64 { match s { Circle(r) => r, Square(w) => w } }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("missing variant `Dot` of enum `Shape`")),
            "{diags:?}"
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

    #[test]
    fn empty_match_exhausts_an_empty_enum_parameter() {
        assert!(
            check(
                "enum Empty {}
                 fn absurd<T>(empty: Empty) -> T { match empty {} }"
            )
            .is_ok()
        );
    }

    #[test]
    fn variant_pattern_must_bind_the_declared_payload() {
        let diags = check(
            "enum Shape { Point, Circle(i64) }
             fn f(s: Shape) -> i64 {
                 match s {
                     Point => 0,
                     Circle(r, extra) => r,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d
                .message
                .contains("variant `Circle` carries 1 payload value(s); the pattern binds 2")),
            "{diags:?}"
        );
    }

    #[test]
    fn variant_pattern_with_the_declared_payload_is_accepted() {
        let r = check(
            "enum Shape { Point, Circle(i64) }
             fn f(s: Shape) -> i64 {
                 match s {
                     Point => 0,
                     Circle(r) => r,
                 }
             }",
        );
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn select_exhaustive_all_variants() {
        assert!(
            check(
                "enum Color { Red, Green, Blue }
             fn main() -> i32 {
                 let cont = select Color {
                     Red => 0 | out⟩,
                     Green => 1 | out⟩,
                     Blue => 2 | out⟩,
                 };
                 cont(Color::Red)
             }"
            )
            .is_ok()
        );
    }

    #[test]
    fn select_missing_variant_rejected() {
        let r = check(
            "enum Color { Red, Green, Blue }
             fn main() -> i32 {
                 let cont = select Color {
                     Red => 0 | out⟩,
                     Green => 1 | out⟩,
                 };
                 cont(Color::Red)
             }",
        );
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("Blue"));
    }

    #[test]
    fn select_duplicate_variant_rejected() {
        let r = check(
            "enum Color { Red, Green, Blue }
             fn main() -> i32 {
                 let cont = select Color {
                     Red => 0 | out⟩,
                     Red => 1 | out⟩,
                     Blue => 2 | out⟩,
                 };
                 cont(Color::Red)
             }",
        );
        assert!(r.is_err());
        assert!(r.unwrap_err().iter().any(|d| d.message.contains("duplicate arm")));
    }

    #[test]
    fn select_unknown_variant_rejected() {
        let r = check(
            "enum Color { Red, Green, Blue }
             fn main() -> i32 {
                 let cont = select Color {
                     Red => 0 | out⟩,
                     Green => 1 | out⟩,
                     Purple => 2 | out⟩,
                 };
                 cont(Color::Red)
             }",
        );
        assert!(r.is_err());
        assert!(r.unwrap_err().iter().any(|d| d.message.contains("unknown variant")));
    }

    #[test]
    fn a_select_that_leaves_out_its_type_is_still_exhaustive_or_not() {
        // The arms name the enum, so coverage is checked against it.
        assert!(
            check(
                "enum Color { Red, Green }
                 fn code(return: -i32) <- Color {
                     select { Red => 0 | return⟩, Green => 1 | return⟩ }
                 }"
            )
            .is_ok()
        );

        let diags = check(
            "enum Color { Red, Green }
             fn code(return: -i32) <- Color { select { Red => 0 | return⟩ } }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("missing variants Green")), "{diags:?}");
    }

    #[test]
    fn select_unknown_enum_rejected() {
        let r = check(
            "fn main() -> i32 {
                 let cont = select Color {
                     Red => 0 | out⟩,
                     Green => 1 | out⟩,
                     Blue => 2 | out⟩,
                 };
                 cont(Color::Red)
             }",
        );
        assert!(r.is_err());
        assert!(r.unwrap_err().iter().any(|d| d.message.contains("unknown type")));
    }
}
