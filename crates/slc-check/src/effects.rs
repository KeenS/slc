//! Effect-row checking.
//!
//! A function declares the effects it may perform — `fn f(…) -> T / {Exn}` —
//! and a bare arrow is the empty row. A function may perform an operation
//! only when its effect is in the function's declared row; a call propagates
//! the callee's row; and `handle e { … }` discharges `E` from `e`'s
//! requirement. `main` has the empty row, so a well-typed program performs no
//! unhandled operation.
//!
//! v1 requires explicit rows: there is no inference, and no row polymorphism
//! (a higher-order function cannot yet forward an argument's effects).

use crate::Diagnostic;
use slc_syntax::ast::{Decl, Expr, Node, Program};
use std::collections::{HashMap, HashSet};

pub fn check_effects(p: &Program) -> Result<(), Vec<Diagnostic>> {
    // Operation → its effect; function/command → its declared row.
    let mut op_effect: HashMap<String, String> = HashMap::new();
    let mut declared: HashMap<String, Vec<String>> = HashMap::new();
    for d in &p.decls {
        match &d.kind {
            Decl::Effect { name, operations } => {
                for op in operations {
                    op_effect.insert(op.name.clone(), name.clone());
                }
            }
            Decl::Fn { name, effects, .. } | Decl::Command { name, effects, .. } => {
                declared.insert(name.clone(), effects.clone());
            }
            _ => {}
        }
    }

    let mut diags = Vec::new();
    for d in &p.decls {
        let (name, effects, body) = match &d.kind {
            Decl::Fn { name, effects, body, .. } => (name, effects, body),
            Decl::Command { name, effects, body, .. } => (name, effects, body),
            _ => continue,
        };
        let allowed: HashSet<&str> = effects.iter().map(String::as_str).collect();
        let mut incurred = HashSet::new();
        collect(body, &op_effect, &declared, &mut incurred);
        for effect in &incurred {
            if !allowed.contains(effect.as_str()) {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` performs `{effect}` but does not declare it; add `/ {{{effect}}}` \
                         to its type, or handle it"
                    ),
                    span: d.span,
                });
            }
        }
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

/// The effects an expression may incur, gathered into `out`.
fn collect(
    e: &Node<Expr>,
    op_effect: &HashMap<String, String>,
    declared: &HashMap<String, Vec<String>>,
    out: &mut HashSet<String>,
) {
    match &e.kind {
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = &callee.kind {
                if let Some(effect) = op_effect.get(name) {
                    out.insert(effect.clone());
                } else if let Some(row) = declared.get(name) {
                    out.extend(row.iter().cloned());
                }
            } else {
                collect(callee, op_effect, declared, out);
            }
            for arg in args {
                collect(arg, op_effect, declared, out);
            }
        }
        // A handler discharges, from the body's requirement, the effects of
        // the operations its clauses answer — inferred from the clause op
        // names, since each operation belongs to one effect.
        Expr::Handle { body, clauses, ret } => {
            let mut inner = HashSet::new();
            collect(body, op_effect, declared, &mut inner);
            for c in clauses {
                if let Some(effect) = op_effect.get(&c.op) {
                    inner.remove(effect);
                }
            }
            out.extend(inner);
            for c in clauses {
                collect(&c.body, op_effect, declared, out);
            }
            if let Some((_, rbody)) = ret {
                collect(rbody, op_effect, declared, out);
            }
        }
        other => {
            for child in other.children() {
                collect(child, op_effect, declared, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_syntax::lexer::lex;
    use slc_syntax::parser::parse;

    fn check(src: &str) -> Result<(), Vec<Diagnostic>> {
        check_effects(&parse(lex(src).unwrap()).unwrap())
    }

    #[test]
    fn an_undeclared_effect_is_rejected() {
        let diags = check(
            "effect Exn { fn throw(m: +String) -> i64; }
             fn bad(x: +i64) -> i64 { throw(\"no\") }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_declared_effect_is_accepted_and_propagates() {
        // `risky` declares Exn; `caller` calls it, so it must declare Exn too.
        assert!(
            check(
                "effect Exn { fn throw(m: +String) -> i64; }
                 fn risky(x: +i64) -> i64 / {Exn} { throw(\"boom\") }
                 fn caller(x: +i64) -> i64 / {Exn} { risky(x) }"
            )
            .is_ok()
        );
        let diags = check(
            "effect Exn { fn throw(m: +String) -> i64; }
             fn risky(x: +i64) -> i64 / {Exn} { throw(\"boom\") }
             fn caller(x: +i64) -> i64 { risky(x) }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`caller` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_negative_function_carries_an_effect_row() {
        // The row sits after the `<-` arrow and is enforced like any other.
        assert!(
            check(
                "effect Log { fn log(m: +String) -> unit; }
                 fn emit(out: -i64) <- i64 / {Log} { log(\"x\"); 42 @ out }"
            )
            .is_ok()
        );
        let diags = check(
            "effect Log { fn log(m: +String) -> unit; }
             fn emit(out: -i64) <- i64 { log(\"x\"); 42 @ out }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`emit` performs `Log`")), "{diags:?}");
    }

    #[test]
    fn handle_discharges_the_effect() {
        // `main` performs nothing: the effect is handled, so its row is empty.
        assert!(
            check(
                "effect Exn { fn throw(m: +String) -> i64; }
                 fn risky(x: +i64) -> i64 / {Exn} { throw(\"boom\") }
                 command main | (exit: -i32) {
                     let r = handle risky(1) { throw(m) resume => 0 - 1, return(n) => n };
                     println(r); 0 @ exit
                 }"
            )
            .is_ok()
        );
    }
}
