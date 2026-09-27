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
            check_params(params, enums, d.span, diags);
            let bindings = declared_bindings(params);
            check_expr(body, enums, &bindings, diags);
        }
        Decl::Command { value_params, continuation_params, body, .. } => {
            check_params(value_params, enums, d.span, diags);
            check_params(continuation_params, enums, d.span, diags);
            let bindings = declared_bindings(value_params);
            check_expr(body, enums, &bindings, diags);
        }
        Decl::Const { value, .. } => check_expr(value, enums, &HashMap::new(), diags),
        Decl::Hand { clauses, ret, .. } => {
            for clause in clauses {
                check_expr(&clause.body, enums, &HashMap::new(), diags);
            }
            if let Some((_, body)) = ret {
                check_expr(body, enums, &HashMap::new(), diags);
            }
        }
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

/// A parameter binds a pattern, and the same two laws hold wherever a binder
/// does: it must match every value of its type, and a continuation must be a
/// name, because a name is what control leaves through.
fn check_params(
    params: &[slc_syntax::ast::Param],
    enums: &Declarations,
    span: slc_syntax::token::Span,
    diags: &mut Vec<Diagnostic>,
) {
    for p in params {
        if p.is_continuation && p.name().is_none() {
            diags.push(Diagnostic {
                message: "a continuation parameter is a name: control leaves through it, \
                          and a pattern has nowhere to leave through"
                    .into(),
                span,
            });
        } else if !is_irrefutable(&p.pattern, enums) {
            diags.push(Diagnostic {
                message: format!(
                    "a parameter binds every value of its type, and {} does not match all \
                     of them; take it apart with `of` in the body",
                    refutable_shape(&p.pattern)
                ),
                span,
            });
        }
    }
}

fn declared_bindings(params: &[slc_syntax::ast::Param]) -> HashMap<String, String> {
    params
        .iter()
        .filter(|param| !param.is_continuation)
        .filter_map(|param| {
            let name = param.name()?.to_string();
            written_type_name(param.ty.as_ref()?).map(|ty| (name, ty))
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
                check_expr(&arm.body, enums, bindings, diags);
            }
        }
        Expr::Select { ty, arms } => {
            // A sum's positions are counted against its type, which the type
            // checker knows and this pass does not.
            if arms.iter().any(|arm| matches!(arm.pattern, Pattern::Inject { .. })) {
                for arm in arms {
                    check_expr(&arm.command, enums, bindings, diags);
                }
                return;
            }
            // `(|)` has no values, so its consumer has no arms.
            if matches!(ty.as_deref().map(|ty| &ty.kind), Some(slc_syntax::ast::TypeExpr::Sum(items)) if items.is_empty())
            {
                if !arms.is_empty() {
                    diags.push(Diagnostic {
                        message: "`(|)` has no values, so `mu (|)` has no arms".into(),
                        span: e.span,
                    });
                }
                for arm in arms {
                    check_expr(&arm.command, enums, bindings, diags);
                }
                return;
            }
            // A `mu` covers each shape of its type exactly once: one arm
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
                    check_branch_coverage("mu", &name, "variant", arms, enums, e.span, diags);
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
                        message: format!("`mu {name}` refers to an unknown type"),
                        span: e.span,
                    });
                }
                // A product has one shape, so it has exactly one arm.
                _ if arms.len() != 1 => {
                    diags.push(Diagnostic {
                        message: format!(
                            "a `mu` over a product has exactly one arm; this one has {}",
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
            // way `mu` covers each variant of its enum.
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
        Expr::Mu { continuation_params, body, .. } => {
            check_params(continuation_params, enums, e.span, diags);
            check_expr(body, enums, bindings, diags);
        }
        Expr::Call { callee, args } => {
            check_expr(callee, enums, bindings, diags);
            for a in args {
                check_expr(a, enums, bindings, diags);
            }
        }
        Expr::Inject { value, .. } => check_expr(value, enums, bindings, diags),
        Expr::Pair(items) | Expr::Bundle(items) | Expr::Par(items) => {
            for i in items {
                check_expr(i, enums, bindings, diags);
            }
        }
        Expr::Let { pattern, value, body, .. } => {
            // A binder stands for every value of its type: there is no other
            // arm to fall to. Anything that can fail to match belongs in a
            // `of`, which says what happens when it does.
            if !is_irrefutable(pattern, enums) {
                diags.push(Diagnostic {
                    message: format!(
                        "`let` binds every value of its type, and {} does not match all of \
                         them; use `of` to say what happens when it does not",
                        refutable_shape(pattern)
                    ),
                    span: e.span,
                });
            }
            check_expr(value, enums, bindings, diags);
            if let Some(b) = body {
                check_expr(b, enums, bindings, diags);
            }
        }
        Expr::Project { base: body, .. } => check_expr(body, enums, bindings, diags),
        Expr::Handle { body, clauses, ret, .. } => {
            check_expr(body, enums, bindings, diags);
            for c in clauses {
                check_expr(&c.body, enums, bindings, diags);
            }
            if let Some((_, rbody)) = ret {
                check_expr(rbody, enums, bindings, diags);
            }
        }
        Expr::Block(exprs) => {
            for ex in exprs {
                check_expr(ex, enums, bindings, diags);
            }
        }
        _ => {}
    }
}

/// Is this variant the only one its enum declares? A `Pattern::Enum` with no
/// variant names the variant itself, so its owner is looked up.
fn sum_of_one(name: &str, variant: &str, enums: &Declarations) -> bool {
    let owner = if variant.is_empty() {
        match enums.variant(name) {
            Some((owner, _)) => owner.clone(),
            None => return false,
        }
    } else {
        name.to_string()
    };
    enums.variants_of(&owner).is_some_and(|variants| variants.len() == 1)
}

/// How to name a binder pattern that can fail, for the reader who wrote it.
fn refutable_shape(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Ident(name) => format!("the variant `{name}`"),
        Pattern::Enum { name, variant, .. } if variant.is_empty() => {
            format!("the variant `{name}`")
        }
        Pattern::Enum { name, variant, .. } => format!("the variant `{name}::{variant}`"),
        Pattern::Data { name, .. } => format!("`{name}`"),
        Pattern::Or(_) => "an or-pattern".into(),
        Pattern::Range { .. } => "a range".into(),
        Pattern::Dtor { dtor, .. } => format!("the request `.{dtor}`"),
        Pattern::Inject { .. } => "an alternative of a sum".into(),
        Pattern::Tuple(_) | Pattern::Bundle(_) => "this pattern".into(),
        _ => "a literal".into(),
    }
}

/// Does this pattern match every value of its type? A sum needs one arm per
/// variant, but a product has a single shape, so one arm covers it.
pub(crate) fn is_irrefutable(pattern: &Pattern, enums: &Declarations) -> bool {
    match pattern {
        Pattern::Wildcard => true,
        // A name that is not a variant is a binding, so it matches anything;
        // one that is covers its enum only when the enum has no other.
        Pattern::Ident(name) => enums.payload_arity(name).is_none() || sum_of_one(name, "", enums),
        Pattern::Binding { pattern, .. } => is_irrefutable(pattern, enums),
        Pattern::Tuple(items) | Pattern::Bundle(items) => {
            items.iter().all(|item| is_irrefutable(item, enums))
        }
        Pattern::Data { name, fields } => {
            enums.declares(name) && fields.iter().all(|(_, pattern)| is_irrefutable(pattern, enums))
        }
        // A sum of one has a single shape, so naming it covers it.
        Pattern::Enum { name, variant, fields } => {
            sum_of_one(name, variant, enums)
                && fields.iter().all(|field| is_irrefutable(field, enums))
        }
        _ => false,
    }
}

/// The name written as a type, if it is a bare declaration name.
fn written_type_name(ty: &slc_syntax::ast::TypeExpr) -> Option<String> {
    match ty {
        slc_syntax::ast::TypeExpr::Base(name) => Some(name.clone()),
        slc_syntax::ast::TypeExpr::Positive(inner) => written_type_name(&inner.kind),
        slc_syntax::ast::TypeExpr::Sum(items) if items.is_empty() => Some("(|)".into()),
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
        let Pattern::Dtor { dtor, arg, .. } = pattern else {
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
/// missing — the coverage law a branch table obeys, shared by `mu` over
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

/// The variant a `mu` arm's pattern selects, if it names one.
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
        Pattern::Inject { pattern, .. } => check_pattern_arity(pattern, enums, span, diags),
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
    // An irrefutable arm covers everything: a wildcard, a plain binding, or
    // the single shape of a product.
    if arms.iter().any(|a| is_irrefutable(&a.pattern, enums)) {
        return;
    }

    for arm in arms {
        check_pattern_arity(&arm.pattern, enums, span, diags);
    }
    // A sum's positions are the type checker's to count.
    if arms.iter().any(|arm| matches!(arm.pattern, Pattern::Inject { .. })) {
        return;
    }

    // Collect enum patterns used: Name(variant, _).
    let mut covered: HashSet<String> = HashSet::new();
    let mut scrutinee_type = match &scrutinee.kind {
        Expr::Ident(name) => bindings.get(name),
        _ => None,
    };
    // `(|)` has no values, so a match on one needs no arm.
    if arms.is_empty() && scrutinee_type.is_some_and(|ty| ty.as_str() == "(|)") {
        return;
    }

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
                    "non-exhaustive of: missing variant{} {} of enum `{enum_name}`",
                    if missing.len() > 1 { "s" } else { "" },
                    missing.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>().join(", ")
                ),
                span,
            });
        }
        return;
    }

    // Without enum coverage information, a match is exhaustive only when it
    // has a wildcard. This conservatively rejects literal-only matches.
    diags.push(Diagnostic { message: "non-exhaustive of: add a `_` arm".into(), span });
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
        check_exhaustiveness(&prog)
    }

    #[test]
    fn a_binder_pattern_must_be_irrefutable() {
        // Tuples, records, single-variant enums and `_` each have one shape.
        assert!(
            check(
                "data Point { x: i64, y: i64 }
                 enum Wrapped { Only(i64) }
                 proc main | (exit: -i32) / {IO} {
                     let (a, b) = (1, 2);
                     let Point { x, y } = Point { x: 3, y: 4 };
                     let Only(n) = Only(5);
                     let _ = 6;
                     <(a, b) | __add | z => (z, x) | __add | z => (z, y) | __add | z => (z, n) | __add | exit>
                 }"
            )
            .is_ok()
        );
        // A sum of many does not, so it belongs in a `of`.
        let diags = check(
            "enum Shape { Circle(i64), Rect(i64, i64) }
             proc main | (exit: -i32) / {IO} { let Circle(r) = Circle(5); <r | exit> }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("does not match all of them")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_parameter_binds_a_pattern_and_an_exit_binds_a_name() {
        assert!(
            check(
                "func skew((a, b): (+i64, +i64), c: +i64) -> i64 { (<(a, c) | __mul | x => (x, b) | __sub) }
                 proc main | (exit: -i32) / {IO} { <((1, 2), 3) | skew | exit> }"
            )
            .is_ok()
        );
        // Control leaves through a name, so an exit cannot be taken apart.
        let diags = check(
            "proc route(n: +i64) | ((a & b): (-i64 & -i64)) { <n | a> }
             proc main | (exit: -i32) / {IO} { <0 | exit> }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("a continuation parameter is a name")),
            "{diags:?}"
        );
    }

    #[test]
    fn exhaustive_with_wildcard() {
        assert!(
            check(
                "enum Color { Red, Green, Blue }
             func f(c: Color) -> i32 { of c { _ => 0 } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn non_exhaustive_detected() {
        let r = check(
            "enum Color { Red, Green, Blue }
             func f(c: Color) -> i32 { of c { Red => 1, Green => 2 } }",
        );
        assert!(r.is_err());
        let msg = &r.unwrap_err()[0].message;
        assert!(msg.contains("non-exhaustive"));
        assert!(msg.contains("Blue"));
    }

    #[test]
    fn a_literal_only_match_is_not_exhaustive() {
        let r = check("func f(c: +i64) -> i64 { of c { 0 => 1 } }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("non-exhaustive"));
    }

    #[test]
    fn binding_around_enum_pattern_still_counts() {
        assert!(
            check(
                "enum Color { Red, Green, Blue }
             func f(c: Color) -> i32 { of c { x @ Red => x, Green => 2, Blue => 3 } }"
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
             func area(s: Shape) -> i64 { of s { Circle(r) => r, Square(w) => w } }"
            )
            .is_ok()
        );

        let diags = check(
            "enum Shape { Circle(i64), Square(i64), Dot }
             func area(s: Shape) -> i64 { of s { Circle(r) => r, Square(w) => w } }",
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
             func f(c: Color) -> i32 { of c { Red => 1, Green => 2, Blue => 3 } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn empty_match_exhausts_an_empty_enum_parameter() {
        assert!(
            check(
                "enum Empty {}
                 func absurd<+T>(empty: Empty) -> T { of empty {} }"
            )
            .is_ok()
        );
    }

    #[test]
    fn variant_pattern_must_bind_the_declared_payload() {
        let diags = check(
            "enum Shape { Point, Circle(i64) }
             func f(s: Shape) -> i64 {
                 of s {
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
             func f(s: Shape) -> i64 {
                 of s {
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
             func main() -> i32 {
                 let cont = mu Color {
                     Red => <0 | out>,
                     Green => <1 | out>,
                     Blue => <2 | out>,
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
             func main() -> i32 {
                 let cont = mu Color {
                     Red => <0 | out>,
                     Green => <1 | out>,
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
             func main() -> i32 {
                 let cont = mu Color {
                     Red => <0 | out>,
                     Red => <1 | out>,
                     Blue => <2 | out>,
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
             func main() -> i32 {
                 let cont = mu Color {
                     Red => <0 | out>,
                     Green => <1 | out>,
                     Purple => <2 | out>,
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
                 func code(return: -i32) <- Color {
                     mu { Red => <0 | return>, Green => <1 | return> }
                 }"
            )
            .is_ok()
        );

        let diags = check(
            "enum Color { Red, Green }
             func code(return: -i32) <- Color { mu { Red => <0 | return> } }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("missing variants Green")), "{diags:?}");
    }

    #[test]
    fn select_unknown_enum_rejected() {
        let r = check(
            "func main() -> i32 {
                 let cont = mu Color {
                     Red => 0 | out>,
                     Green => 1 | out>,
                     Blue => 2 | out>,
                 };
                 cont(Color::Red)
             }",
        );
        assert!(r.is_err());
        assert!(r.unwrap_err().iter().any(|d| d.message.contains("unknown type")));
    }
}
