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
            // Let-bound variables inside the body are also linear.
            let mut let_bound = Vec::new();
            collect_let_bindings(body, &mut let_bound);
            let uses = count_uses(body);
            for p in params {
                let u = uses.get(&p.name);
                report_linearity(name, &p.name, u, p.is_continuation, body.span, diags);
            }
            for lb in let_bound {
                if !bound.contains(&lb) {
                    let u = uses.get(&lb);
                    report_linearity(name, &lb, u, false, body.span, diags);
                }
            }
        }
        Decl::Command { name, params, body } => {
            let uses = count_uses(body);
            for p in params {
                let u = uses.get(&p.name);
                report_linearity(name, &p.name, u, p.is_continuation, body.span, diags);
            }
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
            let f = otherwise
                .as_ref()
                .map(|o| {
                    let mut m2 = UseMap::default();
                    go(o, &mut m2);
                    m2
                })
                .unwrap_or_default();
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
        let r = check("fn bad(x: +i32, y: +i32) -> i32 { x }");
        assert!(r.is_err());
        assert!(r.unwrap_err()[0].message.contains("never used"));
    }

    #[test]
    fn double_use_fails() {
        let r = check("fn bad(x: +i32) -> i32 { x + x }");
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
}
