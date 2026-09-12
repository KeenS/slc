//! Effect-row checking: explicit rows with row variables.
//!
//! A function declares the effects it may perform in its row — `fn f(…) ->
//! T / {Exn}` — and a bare arrow is the empty row, a pure function: the
//! signature tells the whole truth. Row polymorphism is written the way the
//! rest of the language writes generics, explicitly:
//!
//! ```text
//! fn map<A, B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E}
//! ```
//!
//! `E` is a **row variable**, declared like any generic parameter and used
//! with the `..` "rest" spelling; `{Exn, ..E}` extends it. A parameter's
//! arrow type carries the row calling it may incur, and a call instantiates
//! the callee's row variables from the arguments standing at the positions
//! that mention them: `map(half, xs)` sets `E` to `half`'s row, so the call
//! incurs exactly what `half` performs — and passing an effectful function
//! where a rowless arrow is declared is an error at the call.
//!
//! Every declaration is checked locally against its own row; `handle e
//! { … }` discharges the effects of the operations its clauses answer, and
//! `main`'s row must be empty, so a well-typed program performs no
//! unhandled operation.
//!
//! The analysis follows names, conservatively where a function value loses
//! its name: a lambda's body is charged to the declaration that wrote it, a
//! higher-order global passed on as a value contributes its concrete row
//! but no further forwarding, and a function laundered through a `let`
//! binding is not tracked.

use crate::Diagnostic;
use slc_syntax::ast::{Decl, EffectRow, Expr, Node, Program, TypeExpr};
use std::collections::{BTreeSet, HashMap};

/// A row as a set: concrete effects, and the row variables of the
/// declaration in whose scope this row is read.
#[derive(Default, Clone, Debug)]
struct Row {
    effects: BTreeSet<String>,
    tails: BTreeSet<String>,
}

impl Row {
    fn from_ast(row: &EffectRow) -> Row {
        Row {
            effects: row.effects.iter().cloned().collect(),
            tails: row.tails.iter().cloned().collect(),
        }
    }

    fn extend(&mut self, other: &Row) {
        self.effects.extend(other.effects.iter().cloned());
        self.tails.extend(other.tails.iter().cloned());
    }
}

/// A declaration's effect interface: its declared row, and per parameter
/// the row its arrow type carries (empty for a rowless type).
struct Interface {
    row: Row,
    /// Parameter name → the row of its arrow type, in declaration order.
    params: Vec<(String, Row)>,
}

impl Interface {
    /// The positions whose parameter rows mention the row variable `tail`.
    fn positions_of(&self, tail: &str) -> Vec<usize> {
        self.params
            .iter()
            .enumerate()
            .filter(|(_, (_, row))| row.tails.contains(tail))
            .map(|(i, _)| i)
            .collect()
    }
}

/// The row a parameter's written type carries: the row of `(A -> B / {…})`,
/// looked for through the polarity signs.
fn param_row(ty: &TypeExpr) -> Row {
    match ty {
        TypeExpr::Effectful(_, row) => Row::from_ast(row),
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) => param_row(&inner.kind),
        _ => Row::default(),
    }
}

struct Ctx<'a> {
    /// Operation name → the effect it belongs to.
    op_effect: &'a HashMap<String, String>,
    /// Global function/command → its interface.
    interfaces: &'a HashMap<String, Interface>,
    /// The parameters of the declaration under analysis, name → its row.
    params: &'a HashMap<String, Row>,
    diags: &'a mut Vec<Diagnostic>,
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

    let mut decls = Vec::new();
    let mut interfaces: HashMap<String, Interface> = HashMap::new();
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
        let params: Vec<(String, Row)> = params
            .iter()
            .map(|p| (p.name.clone(), p.ty.as_ref().map(param_row).unwrap_or_default()))
            .collect();
        interfaces.insert(name.clone(), Interface { row: Row::from_ast(effects), params });
        decls.push((name.clone(), body, d.span));
    }

    let mut diags = Vec::new();
    for (name, body, span) in &decls {
        let interface = &interfaces[name.as_str()];
        let param_rows: HashMap<String, Row> = interface.params.iter().cloned().collect();
        let mut incurred = Row::default();
        let mut ctx = Ctx {
            op_effect: &op_effect,
            interfaces: &interfaces,
            params: &param_rows,
            diags: &mut diags,
        };
        collect(body, &mut ctx, &mut incurred);

        let allowed = &interface.row;
        for effect in &incurred.effects {
            if !allowed.effects.contains(effect) {
                ctx.diags.push(Diagnostic {
                    message: format!(
                        "`{name}` performs `{effect}` but does not declare it; add \
                         `/ {{{effect}}}` to its type, or handle it"
                    ),
                    span: *span,
                });
            }
        }
        for tail in &incurred.tails {
            if !allowed.tails.contains(tail) {
                ctx.diags.push(Diagnostic {
                    message: format!(
                        "`{name}` performs the row `..{tail}` of a parameter but does not \
                         declare it; add `..{tail}` to its row"
                    ),
                    span: *span,
                });
            }
        }
        if name == "main" && !(allowed.effects.is_empty() && allowed.tails.is_empty()) {
            ctx.diags.push(Diagnostic {
                message: "`main` is the root: its row must be empty, so every effect is \
                          handled before it"
                    .into(),
                span: *span,
            });
        }
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

/// The row a *name* stands for when handed around as a function: an
/// operation's effect, a parameter's declared row, a global's declared row.
fn row_of_name(name: &str, ctx: &Ctx) -> Row {
    if let Some(effect) = ctx.op_effect.get(name) {
        Row { effects: BTreeSet::from([effect.clone()]), tails: BTreeSet::new() }
    } else if let Some(row) = ctx.params.get(name) {
        row.clone()
    } else if let Some(interface) = ctx.interfaces.get(name) {
        // Passed on as a value, a global contributes its concrete row; its
        // own row variables are bound by arguments it has not received.
        Row { effects: interface.row.effects.clone(), tails: BTreeSet::new() }
    } else {
        Row::default()
    }
}

/// The effects an expression may incur, gathered into `out`.
fn collect(e: &Node<Expr>, ctx: &mut Ctx, out: &mut Row) {
    match &e.kind {
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = &callee.kind {
                if let Some(effect) = ctx.op_effect.get(name) {
                    out.effects.insert(effect.clone());
                } else if let Some(row) = ctx.params.get(name) {
                    out.extend(&row.clone());
                } else if let Some(interface) = ctx.interfaces.get(name.as_str()) {
                    out.effects.extend(interface.row.effects.iter().cloned());
                    // Instantiate each row variable of the callee from the
                    // arguments standing at the positions that mention it.
                    for tail in interface.row.tails.clone() {
                        for position in ctx.interfaces[name.as_str()].positions_of(&tail) {
                            if let Some(arg) = args.get(position)
                                && let Expr::Ident(passed) = &arg.kind
                            {
                                let declared = &ctx.interfaces[name.as_str()].params[position].1;
                                let mut arg_row = row_of_name(passed, ctx);
                                // What the parameter's own row already
                                // covers does not flow into the variable.
                                for effect in &declared.effects {
                                    arg_row.effects.remove(effect);
                                }
                                out.extend(&arg_row);
                            }
                        }
                    }
                    // A rowless parameter is a promise of purity: check it.
                    for (position, (param, declared)) in interface.params.iter().enumerate() {
                        if let Some(arg) = args.get(position)
                            && let Expr::Ident(passed) = &arg.kind
                        {
                            let arg_row = row_of_name(passed, ctx);
                            if declared.tails.is_empty() {
                                for effect in &arg_row.effects {
                                    if !declared.effects.contains(effect) {
                                        ctx.diags.push(Diagnostic {
                                            message: format!(
                                                "`{name}` takes `{param}` with{} but `{passed}` \
                                                 performs `{effect}`",
                                                if declared.effects.is_empty() {
                                                    " a pure arrow".to_string()
                                                } else {
                                                    format!(
                                                        " row {{{}}}",
                                                        declared
                                                            .effects
                                                            .iter()
                                                            .cloned()
                                                            .collect::<Vec<_>>()
                                                            .join(", ")
                                                    )
                                                },
                                                effect = effect
                                            ),
                                            span: arg.span,
                                        });
                                    }
                                }
                            }
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
        // names, since each operation belongs to one effect. A row variable
        // cannot be discharged by name: it stays.
        Expr::Handle { body, clauses, ret } => {
            let mut inner = Row::default();
            collect(body, ctx, &mut inner);
            for c in clauses {
                if let Some(effect) = ctx.op_effect.get(&c.op) {
                    inner.effects.remove(effect);
                }
            }
            out.extend(&inner);
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
    fn an_undeclared_effect_is_rejected() {
        let diags =
            check(&format!("{EXN} fn bad(x: +i64) -> i64 {{ throw(\"no\") }}")).unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`bad` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_declared_effect_is_accepted_and_propagates() {
        // `risky` declares Exn; `caller` calls it, so it must declare it too
        // — the check is local to every declaration.
        assert!(
            check(&format!(
                "{EXN} fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 fn caller(x: +i64) -> i64 / {{Exn}} {{ risky(x) }}"
            ))
            .is_ok()
        );
        let diags = check(&format!(
            "{EXN} fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
             fn caller(x: +i64) -> i64 {{ risky(x) }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`caller` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_row_variable_forwards_an_arguments_row() {
        // `app` declares the forwarding explicitly: its row is `f`'s row.
        let src = |main_body: &str| {
            format!(
                "{EXN} fn app<E>(f: (+i64 -> +i64 / {{..E}}), x: +i64) -> i64 / {{..E}} {{ f(x) }}
                 fn inc(x: +i64) -> i64 {{ x + 1 }}
                 fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 command main | (exit: -i32) {{ {main_body}; 0 @ exit }}"
            )
        };
        // A pure argument instantiates E to the empty row.
        assert!(check(&src("println(app(inc, 1))")).is_ok());
        // An effectful one flows into the caller, which must answer for it.
        let diags = check(&src("println(app(risky, 1))")).unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
        // Handled at the call, the row is discharged and `main` stays pure.
        assert!(
            check(&src(
                "let r = handle app(risky, 1) { throw(m) resume => 0 - 1, return(n) => n };
                 println(r)"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_rowless_arrow_is_a_promise_of_purity() {
        // `app` declares `f` pure and no row of its own: passing `risky`
        // is an error at the call site.
        let diags = check(&format!(
            "{EXN} fn app(f: (+i64 -> +i64), x: +i64) -> i64 {{ f(x) }}
             fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
             command main | (exit: -i32) {{ println(app(risky, 1)); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("takes `f` with a pure arrow")
                && d.message.contains("`risky` performs `Exn`")),
            "{diags:?}"
        );
    }

    #[test]
    fn an_undeclared_forwarded_row_is_rejected() {
        // Calling a parameter whose type carries `..E` incurs `..E`; the
        // declaration must carry it too.
        let diags = check(&format!(
            "{EXN} fn app<E>(f: (+i64 -> +i64 / {{..E}}), x: +i64) -> i64 {{ f(x) }}"
        ))
        .unwrap_err();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("performs the row `..E`")
                    && d.message.contains("add `..E`")),
            "{diags:?}"
        );
    }

    #[test]
    fn forwarding_composes_through_the_call_graph() {
        // `twice` forwards through `app`: `E` of `app` instantiated with
        // the row of `g`, itself the variable `..F` of `twice`.
        assert!(
            check(&format!(
                "{EXN} fn app<E>(f: (+i64 -> +i64 / {{..E}}), x: +i64) -> i64 / {{..E}} {{ f(x) }}
                 fn twice<F>(g: (+i64 -> +i64 / {{..F}}), x: +i64) -> i64 / {{..F}} {{
                     app(g, app(g, x))
                 }}
                 fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 command main | (exit: -i32) {{
                     let r = handle twice(risky, 8) {{ throw(m) resume => 0 - 1, return(n) => n }};
                     println(r); 0 @ exit
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn an_operation_passed_as_a_value_carries_its_effect() {
        let diags = check(&format!(
            "{EXN} fn app<E>(f: (+String -> +i64 / {{..E}}), x: +String) -> i64 / {{..E}} {{ f(x) }}
             command main | (exit: -i32) {{ println(app(throw, \"m\")); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_row_extension_covers_the_named_part() {
        // `{Exn, ..E}` on the parameter: Exn is the callee's own business
        // (it declares it), and only the rest flows through E.
        assert!(
            check(&format!(
                "{EXN} fn guard<E>(f: (+i64 -> +i64 / {{Exn, ..E}}), x: +i64) -> i64 / {{Exn, ..E}} {{
                     f(x)
                 }}
                 fn risky(x: +i64) -> i64 / {{Exn}} {{ throw(\"boom\") }}
                 command main | (exit: -i32) {{
                     let r = handle guard(risky, 1) {{ throw(m) resume => 0 - 1, return(n) => n }};
                     println(r); 0 @ exit
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn main_must_be_pure() {
        let diags = check(&format!(
            "{EXN} command main | (exit: -i32) / {{Exn}} {{ println(throw(\"no\")); 0 @ exit }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` is the root")), "{diags:?}");
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
}
