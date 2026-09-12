//! Effect-row checking: inference with row polymorphism.
//!
//! Every function's row is inferred: the checker computes a **summary** per
//! declaration — the concrete effects the body may perform, plus the
//! positions of the parameters whose own effects it forwards. Calling a
//! parameter forwards that parameter's row; calling a global instantiates
//! the callee's summary with the arguments at hand, so a higher-order
//! function performs whatever the function it was given does: `map(f, xs)`
//! carries `f`'s row without declaring anything.
//!
//! A written row — `fn f(…) -> T / {Exn}` — is a *bound*: the inferred
//! concrete row must fit inside it, and the old diagnostic fires where it
//! does not. A bare arrow promises nothing and infers everything; the place
//! purity is enforced is `main`, whose row must come out empty, so a
//! well-typed program performs no unhandled operation. `handle e { … }`
//! discharges, from the body's inferred requirement, the effects of the
//! operations its clauses answer.
//!
//! The analysis is conservative where a function value loses its name: a
//! lambda's body is charged to the declaration that wrote it, and a
//! higher-order global passed on as an argument contributes its concrete
//! row but no further forwarding. A function laundered through a `let`
//! binding is not tracked — the summary follows names, not values.

use crate::Diagnostic;
use slc_syntax::ast::{Decl, Expr, Node, Program};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// A declaration's effect summary. `concrete` maps each effect to the
/// callee it arrived through (`None`: an operation performed directly);
/// `forwards` holds the positions of parameters whose rows flow through.
#[derive(Default, Clone)]
struct Summary {
    concrete: BTreeMap<String, Option<String>>,
    forwards: BTreeSet<usize>,
}

impl Summary {
    fn same_row(&self, other: &Summary) -> bool {
        self.forwards == other.forwards
            && self.concrete.len() == other.concrete.len()
            && self.concrete.keys().eq(other.concrete.keys())
    }
}

struct Ctx<'a> {
    /// Operation name → the effect it belongs to.
    op_effect: &'a HashMap<String, String>,
    /// Global function/command → its current summary.
    summaries: &'a HashMap<String, Summary>,
    /// The parameters of the declaration under analysis, name → position.
    params: &'a HashMap<String, usize>,
}

pub fn check_effects(p: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut op_effect: HashMap<String, String> = HashMap::new();
    for d in &p.decls {
        if let Decl::Effect { name, operations } = &d.kind {
            for op in operations {
                op_effect.insert(op.name.clone(), name.clone());
            }
        }
    }

    // (name, param positions, declared row, body, span) per declaration.
    let mut decls = Vec::new();
    for d in &p.decls {
        let (name, params, effects, body) = match &d.kind {
            Decl::Fn { name, params, effects, body, .. } => {
                (name, params.iter().collect::<Vec<_>>(), effects, body)
            }
            Decl::Command { name, value_params, continuation_params, effects, body, .. } => {
                (name, value_params.iter().chain(continuation_params).collect(), effects, body)
            }
            _ => continue,
        };
        let positions: HashMap<String, usize> =
            params.iter().enumerate().map(|(i, p)| (p.name.clone(), i)).collect();
        decls.push((name.clone(), positions, effects, body, d.span));
    }

    // The call graph may be cyclic, so iterate to a fixpoint: summaries only
    // grow, and the sets are finite, so this terminates.
    let mut summaries: HashMap<String, Summary> =
        decls.iter().map(|(name, ..)| (name.clone(), Summary::default())).collect();
    loop {
        let mut changed = false;
        for (name, positions, _, body, _) in &decls {
            let mut s = Summary::default();
            let ctx = Ctx { op_effect: &op_effect, summaries: &summaries, params: positions };
            collect(body, &ctx, &mut s);
            if !s.same_row(&summaries[name]) {
                summaries.insert(name.clone(), s);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut diags = Vec::new();
    for (name, _, effects, _, span) in &decls {
        let inferred = &summaries[name.as_str()];
        // A written row is a bound; a bare arrow infers, except at `main`,
        // the root, where every effect must have been handled.
        if effects.is_empty() && name != "main" {
            continue;
        }
        let allowed: HashSet<&str> = effects.iter().map(String::as_str).collect();
        for (effect, via) in &inferred.concrete {
            if !allowed.contains(effect.as_str()) {
                let via = match via {
                    Some(callee) => format!(" (via `{callee}`)"),
                    None => String::new(),
                };
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` performs `{effect}`{via} but does not declare it; add \
                         `/ {{{effect}}}` to its type, or handle it"
                    ),
                    span: *span,
                });
            }
        }
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

/// Add to `out` what a name contributes when it stands in function
/// position or is handed to one: an operation brings its effect, a
/// parameter forwards its row, a global brings its concrete row.
fn contribute(name: &str, via_call: bool, ctx: &Ctx, out: &mut Summary) {
    if let Some(effect) = ctx.op_effect.get(name) {
        out.concrete.entry(effect.clone()).or_insert(None);
    } else if let Some(&position) = ctx.params.get(name) {
        out.forwards.insert(position);
    } else if let Some(row) = ctx.summaries.get(name) {
        for effect in row.concrete.keys() {
            let via = via_call.then(|| name.to_string());
            out.concrete.entry(effect.clone()).or_insert(via);
        }
        // The global's own forwarding is not instantiated here: passed on
        // as a value, it contributes only what it performs by itself.
    }
}

/// The effects an expression may incur, gathered into `out`.
fn collect(e: &Node<Expr>, ctx: &Ctx, out: &mut Summary) {
    match &e.kind {
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = &callee.kind {
                contribute(name, true, ctx, out);
                // Row polymorphism: the callee's forwarded positions are
                // instantiated with the arguments standing there.
                if let Some(row) = ctx.summaries.get(name.as_str()) {
                    for &position in &row.forwards {
                        if let Some(arg) = args.get(position)
                            && let Expr::Ident(passed) = &arg.kind
                        {
                            contribute(passed, true, ctx, out);
                        }
                    }
                }
            } else {
                collect(callee, ctx, out);
            }
            for arg in args {
                collect(arg, ctx, out);
            }
        }
        // A handler discharges, from the body's requirement, the effects of
        // the operations its clauses answer — inferred from the clause op
        // names, since each operation belongs to one effect.
        Expr::Handle { body, clauses, ret } => {
            let mut inner = Summary::default();
            collect(body, ctx, &mut inner);
            for c in clauses {
                if let Some(effect) = ctx.op_effect.get(&c.op) {
                    inner.concrete.remove(effect);
                }
            }
            out.concrete.extend(inner.concrete);
            out.forwards.extend(inner.forwards);
            for c in clauses {
                collect(&c.body, ctx, out);
            }
            if let Some((_, rbody)) = ret {
                collect(rbody, ctx, out);
            }
        }
        other => {
            for child in other.children() {
                collect(child, ctx, out);
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

    const EXN: &str = "effect Exn { fn throw(m: +String) -> i64; }\n";

    #[test]
    fn an_undeclared_effect_is_rejected_at_a_written_row() {
        // A written row is a bound the body must fit.
        let diags = check(&format!(
            "{EXN} fn bad(x: +i64) -> i64 / {{}} {{ throw(\"no\") }}
             command main | (exit: -i32) {{ 0 @ exit }}"
        ));
        // `/ {}` may not parse as an empty row; the enforced boundary that
        // always exists is `main`.
        let diags = match diags {
            Err(d) => d,
            Ok(()) => check(&format!(
                "{EXN} command main | (exit: -i32) {{ println(throw(\"no\")); 0 @ exit }}"
            ))
            .unwrap_err(),
        };
        assert!(diags.iter().any(|d| d.message.contains("performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_declared_effect_is_accepted_and_a_bare_arrow_infers() {
        // `risky` declares Exn; `caller` writes nothing and simply carries
        // the row — the error surfaces at `main`, the pure boundary.
        assert!(
            check(&format!(
                "{EXN} fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 fn caller(x: +i64) -> i64 {{ risky(x) }}"
            ))
            .is_ok()
        );
        let diags = check(&format!(
            "{EXN} fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
             fn caller(x: +i64) -> i64 {{ risky(x) }}
             command main | (exit: -i32) {{ println(caller(1)); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`main` performs `Exn` (via `caller`)")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_written_row_is_a_bound() {
        let diags = check(&format!(
            "{EXN} effect Log {{ fn log(m: +String) -> unit; }}
             fn risky(x: +i64) -> i64 / {{Log}} {{ log(\"x\"); throw(\"boom\") }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`risky` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_higher_order_function_forwards_its_arguments_row() {
        // `app` calls its parameter: its row is whatever `f`'s is. Passing
        // the pure `inc` incurs nothing; passing `risky` reaches `main`.
        let pure = check(&format!(
            "{EXN} fn app(f: (+i64 -> +i64), x: +i64) -> i64 {{ f(x) }}
             fn inc(x: +i64) -> i64 {{ x + 1 }}
             command main | (exit: -i32) {{ println(app(inc, 1)); 0 @ exit }}"
        ));
        assert!(pure.is_ok(), "{pure:?}");

        let diags = check(&format!(
            "{EXN} fn app(f: (+i64 -> +i64), x: +i64) -> i64 {{ f(x) }}
             fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
             command main | (exit: -i32) {{ println(app(risky, 1)); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`main` performs `Exn` (via `risky`)")),
            "{diags:?}"
        );
    }

    #[test]
    fn an_operation_passed_as_a_value_carries_its_effect() {
        let diags = check(&format!(
            "{EXN} fn app(f: (+String -> +i64), x: +String) -> i64 {{ f(x) }}
             command main | (exit: -i32) {{ println(app(throw, \"m\")); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn forwarding_composes_through_the_call_graph() {
        // `twice` forwards through `app`: two hops of instantiation.
        let diags = check(&format!(
            "{EXN} fn app(f: (+i64 -> +i64), x: +i64) -> i64 {{ f(x) }}
             fn twice(g: (+i64 -> +i64), x: +i64) -> i64 {{ app(g, app(g, x)) }}
             fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
             command main | (exit: -i32) {{ println(twice(risky, 1)); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn handle_discharges_the_effect_including_a_forwarded_one() {
        // Handling around a direct call, and around a higher-order call
        // whose row arrived by forwarding: both leave `main` pure.
        assert!(
            check(&format!(
                "{EXN} fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 command main | (exit: -i32) {{
                     let r = handle risky(1) {{ throw(m) resume => 0 - 1, return(n) => n }};
                     println(r); 0 @ exit
                 }}"
            ))
            .is_ok()
        );
        assert!(
            check(&format!(
                "{EXN} fn app(f: (+i64 -> +i64), x: +i64) -> i64 {{ f(x) }}
                 fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 command main | (exit: -i32) {{
                     let r = handle app(risky, 1) {{ throw(m) resume => 0 - 1, return(n) => n }};
                     println(r); 0 @ exit
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_negative_function_carries_an_effect_row() {
        // The row sits after the `<-` arrow and is enforced like any other.
        let diags = check(
            "effect Log { fn log(m: +String) -> unit; }
             fn emit(out: -i64) <- i64 / {} { log(\"x\"); 42 @ out }
             command main | (exit: -i32) { println(mu i64 { k <= emit(k) }); 0 @ exit }",
        );
        let diags = match diags {
            Err(d) => d,
            Ok(()) => check(
                "effect Log { fn log(m: +String) -> unit; }
                 fn emit(out: -i64) <- i64 { log(\"x\"); 42 @ out }
                 command main | (exit: -i32) { println(mu i64 { k <= emit(k) }); 0 @ exit }",
            )
            .unwrap_err(),
        };
        assert!(diags.iter().any(|d| d.message.contains("performs `Log`")), "{diags:?}");
    }
}
