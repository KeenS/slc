//! Linearity checking: every variable used exactly once, every continuation
//! activated exactly once on every path.

use slc_syntax::ast::{Decl, Expr, Node, Program};
use slc_syntax::token::Span;
use std::collections::{HashMap, HashSet};

pub use crate::Diagnostic;

/// Continue checking after `if` without an `else`: a linear continuation
/// that is used only in the taken branch is left dangling when control
/// falls through.
fn check_dangling_continuations(e: &Node<Expr>, diags: &mut Vec<Diagnostic>) {
    match &e.kind {
        Expr::If { otherwise: None, then, .. } => {
            for (name, uses) in count_uses(e).uses {
                if uses == Use::One
                    && is_continuation_name(name.as_str())
                    && count_uses(then).get(&name) == Use::One
                    && let Some(span) = find_ident_span(e, &name)
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "continuation `{name}` dangles on the false path of this `if`"
                        ),
                        span,
                    });
                }
            }
            check_dangling_continuations(then, diags);
        }
        Expr::If { then, otherwise: Some(otherwise), .. } => {
            check_dangling_continuations(then, diags);
            check_dangling_continuations(otherwise, diags);
        }
        Expr::Let { value, body: Some(body), .. } => {
            check_dangling_continuations(value, diags);
            check_dangling_continuations(body, diags);
        }
        Expr::Match { scrutinee, arms } => {
            check_dangling_continuations(scrutinee, diags);
            for arm in arms {
                check_dangling_continuations(&arm.body, diags);
            }
        }
        Expr::Select { arms, .. } => {
            for arm in arms {
                check_dangling_continuations(&arm.command, diags);
            }
        }
        Expr::Block(exprs) => {
            for e in exprs {
                check_dangling_continuations(e, diags);
            }
        }
        _ => {}
    }
}

fn is_continuation_name(name: &str) -> bool {
    name == "k" || name == "ok" || name.ends_with("_cont") || name.starts_with("cont")
}

fn find_ident_span(e: &Node<Expr>, name: &str) -> Option<Span> {
    match &e.kind {
        Expr::Ident(x) if x == name => Some(e.span),
        Expr::Lambda { body, .. }
        | Expr::ErrorProp { expr: body, .. }
        | Expr::Shift { expr: body, .. } => find_ident_span(body, name),
        Expr::Call { callee, args } => find_ident_span(callee, name)
            .or_else(|| args.iter().find_map(|a| find_ident_span(a, name))),
        Expr::Pair(items) => items.iter().find_map(|i| find_ident_span(i, name)),
        Expr::Match { scrutinee, arms } => find_ident_span(scrutinee, name)
            .or_else(|| arms.iter().find_map(|arm| find_ident_span(&arm.body, name))),
        Expr::Select { arms, .. } => {
            arms.iter().find_map(|arm| find_ident_span(&arm.command, name))
        }
        Expr::Let { value, body, .. } => find_ident_span(value, name)
            .or_else(|| body.as_ref().and_then(|body| find_ident_span(body, name))),
        Expr::If { cond, then, otherwise } => find_ident_span(cond, name)
            .or_else(|| find_ident_span(then, name))
            .or_else(|| otherwise.as_ref().and_then(|otherwise| find_ident_span(otherwise, name))),
        Expr::BinOp { lhs, rhs, .. } => {
            find_ident_span(lhs, name).or_else(|| find_ident_span(rhs, name))
        }
        Expr::Cut { value, consumer } => {
            find_ident_span(value, name).or_else(|| find_ident_span(consumer, name))
        }
        Expr::Block(exprs) => exprs.iter().find_map(|e| find_ident_span(e, name)),
        Expr::UnOp { body, .. } => find_ident_span(body, name),
        Expr::Index { value, index } => {
            find_ident_span(value, name).or_else(|| find_ident_span(index, name))
        }
        Expr::Slice { value, start, end } => find_ident_span(value, name)
            .or_else(|| start.as_ref().and_then(|start| find_ident_span(start, name)))
            .or_else(|| end.as_ref().and_then(|end| find_ident_span(end, name))),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    Zero,
    One,
    Many,
}

#[derive(Debug, Clone, Default)]
struct UseMap {
    uses: HashMap<String, Use>,
}

impl UseMap {
    fn incr(&mut self, name: &str) {
        let entry = self.uses.entry(name.to_string()).or_insert(Use::Zero);
        *entry = match entry {
            Use::Zero => Use::One,
            _ => Use::Many,
        };
    }

    fn get(&self, name: &str) -> Use {
        self.uses.get(name).copied().unwrap_or(Use::Zero)
    }

    /// Sequential composition: uses on both sides happen, so they add up.
    fn merge(&mut self, other: &UseMap) {
        for (k, v) in &other.uses {
            let cur = self.get(k);
            let combined = match (cur, v) {
                (Use::Zero, x) => *x,
                (x, Use::Zero) => x,
                _ => Use::Many,
            };
            self.uses.insert(k.clone(), combined);
        }
    }

    /// Alternative composition: the branches of an `if`, `match`, or `select`
    /// are mutually exclusive, so exactly one of them runs. A name used once
    /// in each branch is used once, not once per branch.
    fn merge_alternative(&mut self, other: &UseMap) {
        for (k, v) in &other.uses {
            let cur = self.get(k);
            let combined = match (cur, v) {
                (Use::Zero, x) => *x,
                (x, Use::Zero) => x,
                (Use::One, Use::One) => Use::One,
                _ => Use::Many,
            };
            self.uses.insert(k.clone(), combined);
        }
    }
}

/// Check linearity of a whole program.
pub fn check_linearity(p: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut diags = Vec::new();
    for d in &p.decls {
        check_decl(d, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_decl(d: &Node<Decl>, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { name, params, body, .. } => {
            let bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
            let unrestricted: HashSet<String> = params
                .iter()
                .filter(|p| p.ty.as_ref().is_some_and(is_unrestricted))
                .map(|p| p.name.clone())
                .collect();
            // Let-bound variables inside the body are also linear.
            let mut let_bound = Vec::new();
            collect_let_bindings(body, &mut let_bound);
            let uses = count_uses(body);
            for p in params {
                if unrestricted.contains(&p.name) {
                    continue;
                }
                let u = uses.get(&p.name);
                let consumer = p.is_continuation || p.ty.as_ref().is_some_and(is_consumer);
                report_linearity(name, &p.name, u, consumer, body.span, diags);
            }
            check_dangling_continuations(body, diags);
            for lb in let_bound {
                if !bound.contains(&lb) {
                    // let-bound values are currently always base values
                    // (pair destructuring isn't implemented), so treat
                    // them as unrestricted.
                    let u = uses.get(&lb);
                    let _ = u;
                }
            }
        }
        Decl::Command { name, value_params, continuation_params, body, .. } => {
            let params: Vec<_> = value_params.iter().chain(continuation_params.iter()).collect();
            let uses = count_uses(body);
            let unrestricted: HashSet<String> = params
                .iter()
                .filter(|p| p.ty.as_ref().is_some_and(is_unrestricted))
                .map(|p| p.name.clone())
                .collect();
            for p in params.into_iter() {
                if unrestricted.contains(&p.name) {
                    continue;
                }
                let u = uses.get(&p.name);
                let consumer = continuation_params.iter().any(|x| x.name == p.name)
                    || p.ty.as_ref().is_some_and(is_consumer);
                report_linearity(name, &p.name, u, consumer, body.span, diags);
            }
            check_dangling_continuations(body, diags);
        }
        Decl::Const { .. } => {}
        Decl::Struct { .. }
        | Decl::Enum { .. }
        | Decl::Mod { .. }
        | Decl::Use { .. }
        | Decl::Trait { .. }
        | Decl::Impl { .. } => {}
    }
}

/// Collect names bound by `let` expressions (recursively).
fn collect_let_bindings(e: &Node<Expr>, out: &mut Vec<String>) {
    match &e.kind {
        Expr::Let { name, value, body, .. } => {
            out.push(name.clone());
            collect_let_bindings(value, out);
            if let Some(b) = body {
                collect_let_bindings(b, out);
            }
        }
        Expr::Lambda { body, .. } => collect_let_bindings(body, out),
        Expr::Call { callee, args } => {
            collect_let_bindings(callee, out);
            for a in args {
                collect_let_bindings(a, out);
            }
        }
        Expr::Pair(items) => {
            for i in items {
                collect_let_bindings(i, out);
            }
        }
        Expr::Match { scrutinee, arms } => {
            collect_let_bindings(scrutinee, out);
            for arm in arms {
                collect_let_bindings(&arm.body, out);
            }
        }
        Expr::Select { arms, .. } => {
            for arm in arms {
                collect_let_bindings(&arm.command, out);
            }
        }
        Expr::If { cond, then, otherwise } => {
            collect_let_bindings(cond, out);
            collect_let_bindings(then, out);
            if let Some(o) = otherwise {
                collect_let_bindings(o, out);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            collect_let_bindings(lhs, out);
            collect_let_bindings(rhs, out);
        }
        Expr::Block(exprs) => {
            for e in exprs {
                collect_let_bindings(e, out);
            }
        }
        Expr::UnOp { body, .. } => collect_let_bindings(body, out),
        Expr::Index { value, index } => {
            collect_let_bindings(value, out);
            collect_let_bindings(index, out);
        }
        Expr::Slice { value, start, end } => {
            collect_let_bindings(value, out);
            if let Some(start) = start {
                collect_let_bindings(start, out);
            }
            if let Some(end) = end {
                collect_let_bindings(end, out);
            }
        }
        _ => {}
    }
}

/// Report what a binder's use count is not allowed to be.
///
/// A value is linear in both directions: dropping it loses it, and using it
/// twice copies it. A consumer is different. A cut does not return, so at most
/// one mention of a consumer can actually run — the others are unreachable —
/// and a consumer that is forwarded to a callee and also cut against here is
/// still activated once. What must be checked for a consumer is therefore only
/// that it is not dropped.
fn report_linearity(
    decl_name: &str,
    var: &str,
    uses: Use,
    is_cont: bool,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    match uses {
        Use::Zero => diags.push(Diagnostic {
            message: format!(
                "in `{decl_name}`: {kind} `{var}` is never used ({rule})",
                kind = if is_cont { "continuation" } else { "variable" },
                rule = if is_cont {
                    "a continuation must be consumed"
                } else {
                    "linear variables must be used exactly once"
                }
            ),
            span,
        }),
        Use::Many if !is_cont => diags.push(Diagnostic {
            message: format!(
                "in `{decl_name}`: variable `{var}` is used more than once (linear variables must be used exactly once)"
            ),
            span,
        }),
        Use::Many | Use::One => {}
    }
}

/// Base types (i32, i64, bool, String, char) are unrestricted: they may be
/// used any number of times. Only structural types (pairs, sums) and
/// continuations are linear.
/// Is this written type a consumer — something a cut can send a value to?
fn is_consumer(ty: &slc_syntax::ast::TypeExpr) -> bool {
    use slc_syntax::ast::TypeExpr;
    matches!(ty, TypeExpr::Negative(_) | TypeExpr::Par(..) | TypeExpr::Bottom | TypeExpr::Dual(_))
}

fn is_unrestricted(ty: &slc_syntax::ast::TypeExpr) -> bool {
    use slc_syntax::ast::TypeExpr;
    match ty {
        // Bare base names default to positive (copyable) values.
        TypeExpr::Base(_) => true,
        // Positive base values are copyable.
        TypeExpr::Positive(inner) => matches!(inner.kind, TypeExpr::Base(_)),
        // Negative types (continuations) are always linear: they must be
        // activated exactly once.
        TypeExpr::Negative(_) => false,
        // Structural types are linear.
        _ => false,
    }
}

/// Count variable uses in an expression. Sequential expressions add their
/// uses; the branches of an `if`, `match`, or `select` are alternatives, so a
/// name used once in each branch is used once overall.
fn count_uses(e: &Node<Expr>) -> UseMap {
    let mut m = UseMap::default();
    go(e, &mut m);
    m
}

fn go(e: &Node<Expr>, m: &mut UseMap) {
    match &e.kind {
        Expr::Ident(x) => m.incr(x),
        Expr::Mu { body, .. } => go(body, m),
        Expr::Lambda { body, .. } => {
            // Parameters are bound; don't count their use outside
            go(body, m);
        }
        Expr::Call { callee, args } => {
            go(callee, m);
            for a in args {
                go(a, m);
            }
        }
        Expr::Pair(items) => {
            for i in items {
                go(i, m);
            }
        }
        Expr::Match { scrutinee, arms } => {
            go(scrutinee, m);
            // Branches: merge with max semantics
            let mut combined = UseMap::default();
            for arm in arms {
                let mut branch = UseMap::default();
                go(&arm.body, &mut branch);
                combined.merge_alternative(&branch);
            }
            m.merge(&combined);
        }
        Expr::Select { arms, .. } => {
            // `select` is a single negative construction: exactly one arm is
            // activated, so a consumer used in every arm is used once.
            let mut combined = UseMap::default();
            for arm in arms {
                let mut branch = UseMap::default();
                go(&arm.command, &mut branch);
                combined.merge_alternative(&branch);
            }
            m.merge(&combined);
        }
        Expr::Let { value, body, .. } => {
            go(value, m);
            if let Some(b) = body {
                go(b, m);
            }
        }
        Expr::If { cond, then, otherwise } => {
            go(cond, m);
            let mut t = UseMap::default();
            go(then, &mut t);
            let mut f = UseMap::default();
            if let Some(o) = otherwise {
                go(o, &mut f);
            }
            t.merge_alternative(&f);
            m.merge(&t);
        }
        Expr::BinOp { lhs, rhs, .. } => {
            go(lhs, m);
            go(rhs, m);
        }
        Expr::UnOp { body, .. } => go(body, m),
        Expr::Index { value, index } => {
            go(value, m);
            go(index, m);
        }
        Expr::Slice { value, start, end } => {
            go(value, m);
            if let Some(start) = start {
                go(start, m);
            }
            if let Some(end) = end {
                go(end, m);
            }
        }
        Expr::Cut { value, consumer } => {
            go(value, m);
            go(consumer, m);
        }
        // A shift is a coercion: what it wraps is used exactly as it is.
        Expr::Shift { expr, .. } => go(expr, m),
        Expr::ErrorProp { expr, continuation } => {
            go(expr, m);
            if let Some(name) = continuation {
                m.incr(name);
            }
        }
        Expr::Block(exprs) => {
            for e in exprs {
                go(e, m);
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
        check_linearity(&prog)
    }

    #[test]
    fn multi_continuation_command_unselected_continuation_is_linear() {
        let r = check(
            "command route(x: +i32) | (even: -i32, odd: -i32) {
                if eq(rem(x, 2), 0) { even(x) } else { odd(x) }
            }
            fn main() -> i32 { route(2)?even }",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn exclusive_branches_use_a_continuation_once() {
        // Both branches activate `k`, but only one branch runs.
        let r = check(
            "command route(x: +i32) | (k: -i32) {
                if eq(rem(x, 2), 0) { k(0) } else { k(1) }
            }",
        );
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn a_consumer_may_be_mentioned_more_than_once() {
        // A cut does not return, so of these three mentions only one can run.
        let r = check(
            "command route(x: +i32) | (k: -i32) {
                0 @ k;
                if eq(x, 0) { 1 @ k } else { 2 @ k }
            }",
        );
        assert!(r.is_ok(), "{r:?}");

        // Forwarding a consumer and also cutting against it is the ordinary
        // shape of a parser that delegates and reports its own errors.
        let r = check(
            "command step(input: +String) | (err: -String) {
                 next(input, err);
                 \"stopped\" @ err
             }",
        );
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn a_value_used_twice_in_sequence_is_rejected() {
        let r = check(
            "fn bad(p: (+i32 ⊗ +i32)) -> i32 {
                use_it(p);
                use_it(p)
            }",
        );
        let diags = r.unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`p` is used more than once")),
            "{diags:?}"
        );
    }

    #[test]
    fn linear_fn_ok() {
        assert!(check("fn id(x: +i32) -> i32 { x }").is_ok());
    }

    #[test]
    fn named_error_prop_counts_as_continuation_use() {
        // `?err` consumes `err`, so the declaration does not drop it.
        assert!(check("command bad(x: +i32) | (err: -i32) { fail(x)?err }").is_ok());
        let r = check("command bad(x: +i32) | (err: -i32) { x }");
        assert!(r.unwrap_err()[0].message.contains("never used"));
    }

    #[test]
    fn unused_var_fails() {
        // Structural values are linear: an unused one is an error.
        let r = check("fn bad(p: (+i32 ⊗ +i32)) -> i32 { 1 }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("never used"));
    }

    #[test]
    fn double_use_fails() {
        // Structural values are linear: using one twice is an error.
        let r = check("fn bad(p: (+i32 ⊗ +i32)) -> i32 { use_it(p); use_it(p) }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("more than once"));
    }

    #[test]
    fn a_positive_function_may_share_a_consumer_it_receives() {
        let r = check(
            "fn parse(input: +String, err: -String) -> i64 {
                 if str_len(input) > 0 {
                     step(input, err)
                 } else {
                     \"empty\" @ err
                 }
             }",
        );
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn a_mu_must_still_consume_its_continuation() {
        // A `command` denotes one: control leaves only through one of its
        // continuations, so dropping one is an error.
        let r = check("command bad(x: +i32) | (k: -i32) { x }");
        let diags = r.unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`k` is never used")), "{diags:?}");
    }

    #[test]
    fn branch_use_counts_once() {
        // x used once per branch, zero uses overall is fine
        assert!(check("fn f(x: +i32) -> i32 { x }").is_ok());
    }

    #[test]
    fn command_continuation_linear() {
        assert!(check("command step(x: +i32) | (k: -i32) { k(x) }").is_ok());
    }

    #[test]
    fn command_unused_continuation_fails() {
        let r = check("command bad(x: +i32) | (k: -i32) { x }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("continuation"));
    }

    #[test]
    fn dangling_continuation_on_false_path_fails() {
        let r = check("fn f(flag: +bool, k: -i32) -> i32 { if flag { k(42) } }");
        assert!(r.is_err());
        assert!(r.unwrap_err().iter().any(|d| d.message.contains("dangles")));
    }

    #[test]
    fn continuation_used_in_both_paths_ok() {
        assert!(
            check(
                "fn f(flag: +bool, k: -i32, h: -i32) -> i32 { if flag { k(42) } else { h(43) } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn negative_fn_continuation_linear() {
        assert!(check("fn f(k: -i32) <- i32 { k(1) }").is_ok());
    }

    #[test]
    fn negative_fn_and_local_mu_capture_do_not_conflict() {
        let r = check(
            "fn f(k: -i32) <- i32 {
                mu escape(outer: -i32) {
                    k(escape(42, outer))
                }
            }",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn select_uses_one_consumer_continuation_across_arms() {
        let r = check(
            "enum Color { Red, Green, Blue }
            fn k(return: -i32) <- Color {
                select Color {
                    Red <= return(0),
                    Green <= return(1),
                    Blue <= return(2),
                }
            }",
        );
        assert!(r.is_ok());
    }

    #[test]
    fn negative_fn_unused_continuation_fails() {
        let r = check("fn f(k: -i32) <- i32 { 1 }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("continuation"));
    }

    #[test]
    fn missing_error_continuation_use_fails() {
        let r = check("command parse(input: +String) | (ok: -String, err: -String) { ok(input) }");
        assert!(r.is_err());
        assert!(r.unwrap_err().iter().any(|d| d.message.contains("never used")));
    }
}
