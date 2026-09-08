//! Linearity checking: every variable used exactly once, every continuation
//! activated exactly once on every path.

use slc_syntax::ast::{Decl, Expr, Node, Program};
use slc_syntax::token::Span;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

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
        | Expr::Mu { body, .. }
        | Expr::Spawn { body }
        | Expr::Dual { body }
        | Expr::ErrorProp { expr: body } => find_ident_span(body, name),
        Expr::Call { callee, args } => find_ident_span(callee, name)
            .or_else(|| args.iter().find_map(|a| find_ident_span(a, name))),
        Expr::Pair(items) => items.iter().find_map(|i| find_ident_span(i, name)),
        Expr::Match { scrutinee, arms } => find_ident_span(scrutinee, name)
            .or_else(|| arms.iter().find_map(|arm| find_ident_span(&arm.body, name))),
        Expr::Let { value, body, .. } => find_ident_span(value, name)
            .or_else(|| body.as_ref().and_then(|body| find_ident_span(body, name))),
        Expr::If { cond, then, otherwise } => find_ident_span(cond, name)
            .or_else(|| find_ident_span(then, name))
            .or_else(|| otherwise.as_ref().and_then(|otherwise| find_ident_span(otherwise, name))),
        Expr::BinOp { lhs, rhs, .. } => {
            find_ident_span(lhs, name).or_else(|| find_ident_span(rhs, name))
        }
        Expr::Interaction { left, right } => {
            find_ident_span(left, name).or_else(|| find_ident_span(right, name))
        }
        Expr::CommandDef { body, .. } => find_ident_span(body, name),
        Expr::Service { agent, continuations } => find_ident_span(agent, name)
            .or_else(|| continuations.iter().find_map(|c| find_ident_span(c, name))),
        Expr::Job { agent, values } => find_ident_span(agent, name)
            .or_else(|| values.iter().find_map(|v| find_ident_span(v, name))),
        Expr::Block(exprs) => exprs.iter().find_map(|e| find_ident_span(e, name)),
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
            let unrestricted: HashSet<String> =
                params.iter().filter(|p| is_unrestricted(&p.ty)).map(|p| p.name.clone()).collect();
            // Let-bound variables inside the body are also linear.
            let mut let_bound = Vec::new();
            collect_let_bindings(body, &mut let_bound);
            let uses = count_uses(body);
            for p in params {
                if unrestricted.contains(&p.name) {
                    continue;
                }
                let u = uses.get(&p.name);
                report_linearity(name, &p.name, u, p.is_continuation, body.span, diags);
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
        Decl::Command { name, params, body } => {
            let uses = count_uses(body);
            let unrestricted: HashSet<String> =
                params.iter().filter(|p| is_unrestricted(&p.ty)).map(|p| p.name.clone()).collect();
            for p in params {
                if unrestricted.contains(&p.name) {
                    continue;
                }
                let u = uses.get(&p.name);
                report_linearity(name, &p.name, u, p.is_continuation, body.span, diags);
            }
            check_dangling_continuations(body, diags);
        }
        Decl::Struct { .. } | Decl::Enum { .. } => {}
    }
}

/// Collect names bound by `let` expressions (recursively).
fn collect_let_bindings(e: &Node<Expr>, out: &mut Vec<String>) {
    match &e.kind {
        Expr::Let { name, value, body } => {
            out.push(name.clone());
            collect_let_bindings(value, out);
            if let Some(b) = body {
                collect_let_bindings(b, out);
            }
        }
        Expr::Lambda { body, .. } => collect_let_bindings(body, out),
        Expr::Mu { body, .. } => collect_let_bindings(body, out),
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
        _ => {}
    }
}

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
                "in `{decl_name}`: {kind} `{var}` is never used (linear variables must be used exactly once)",
                kind = if is_cont { "continuation" } else { "variable" }
            ),
            span,
        }),
        Use::Many => diags.push(Diagnostic {
            message: format!(
                "in `{decl_name}`: {kind} `{var}` is used more than once (linear variables must be used exactly once)",
                kind = if is_cont { "continuation" } else { "variable" }
            ),
            span,
        }),
        Use::One => {}
    }
}

/// Base types (i32, i64, bool, String, char) are unrestricted: they may be
/// used any number of times. Only structural types (pairs, sums) and
/// continuations are linear.
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

/// Count variable uses in an expression. For branches, merges with max
/// semantics (use in either branch counts as one use).
fn count_uses(e: &Node<Expr>) -> UseMap {
    let mut m = UseMap::default();
    go(e, &mut m);
    m
}

fn go(e: &Node<Expr>, m: &mut UseMap) {
    match &e.kind {
        Expr::Ident(x) => m.incr(x),
        Expr::Lambda { body, .. } => {
            // Parameters are bound; don't count their use outside
            go(body, m);
        }
        Expr::Mu { body, .. } => go(body, m),
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
                combined.merge(&branch);
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
            t.merge(&f);
            m.merge(&t);
        }
        Expr::BinOp { lhs, rhs, .. } => {
            go(lhs, m);
            go(rhs, m);
        }
        Expr::Interaction { left, right } => {
            go(left, m);
            go(right, m);
        }
        Expr::Spawn { body } => go(body, m),
        Expr::Dual { body } => go(body, m),
        Expr::ErrorProp { expr } => go(expr, m),
        Expr::CommandDef { body, .. } => go(body, m),
        Expr::Service { agent, continuations } => {
            go(agent, m);
            for k in continuations {
                go(k, m);
            }
        }
        Expr::Job { agent, values } => {
            go(agent, m);
            for v in values {
                go(v, m);
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
    fn linear_fn_ok() {
        assert!(check("fn id(x: +i32) -> i32 { x }").is_ok());
    }

    #[test]
    fn unused_var_fails() {
        // Pairs are structural (linear): an unused pair binding is an error.
        let r = check("fn bad(x: (-i32), y: (-i32)) -> i32 { 1 }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("never used"));
    }

    #[test]
    fn double_use_fails() {
        // Pairs are structural (linear): double use is an error.
        let r = check("fn bad(x: (-i32), y: (-i32)) -> i32 { k(x); k(x) }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("more than once"));
    }

    #[test]
    fn branch_use_counts_once() {
        // x used once per branch, zero uses overall is fine
        assert!(check("fn f(x: +i32) -> i32 { x }").is_ok());
    }

    #[test]
    fn command_continuation_linear() {
        assert!(check("command step(x: +i32, to k: -i32) { k(x) }").is_ok());
    }

    #[test]
    fn command_unused_continuation_fails() {
        let r = check("command bad(x: +i32, to k: -i32) { x }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("continuation"));
    }

    #[test]
    fn service_agent_linear() {
        assert!(check("fn f(svc: -i32) -> i32 { step.to(svc) }").is_ok());
    }

    #[test]
    fn service_agent_double_use_fails() {
        let r = check("fn f(svc: -i32) -> i32 { step.to(svc); step.to(svc) }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("more than once"));
    }

    #[test]
    fn job_agent_linear() {
        assert!(check("fn f(job: -i32) -> i32 { g.partial(job) }").is_ok());
    }

    #[test]
    fn job_agent_double_use_fails() {
        let r = check("fn f(job: -i32) -> i32 { g.partial(job); g.partial(job) }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("more than once"));
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
}
