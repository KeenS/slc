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
//! `main`'s row is `{IO}` or empty — the runtime is the handler for `IO`
//! and for nothing else — so a well-typed program performs no operation
//! that cannot be answered.
//!
//! A pipeline stage is a call, and charges as one. Only the first stage's
//! argument is syntax, so that is the one whose row variables can be
//! instantiated; the closing consumer is not applied and charges nothing.
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
    /// The latent row of the RESULT type — `-> (-A / {..E})`: what running
    /// the returned value (feeding a consumer, applying a function) may
    /// perform, as opposed to what the call itself does.
    latent: Row,
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

/// The latent row of a result type — what running the returned value may
/// perform. The same shape as `param_row`; the name marks the reading.
fn return_latent(ty: &TypeExpr) -> Row {
    param_row(ty)
}

/// Split a body into its returned suspended literal — a `fn`, `select`, or
/// `mu` in tail position, whose effects belong to the result's latent row —
/// and the rest. Without a latent row there is nothing to divert.
fn returned_literal<'a>(
    body: &'a Node<Expr>,
    latent: &Row,
) -> (Option<&'a Node<Expr>>, Vec<&'a Node<Expr>>) {
    if latent.effects.is_empty() && latent.tails.is_empty() {
        return (None, vec![body]);
    }
    let (tail, rest): (&Node<Expr>, &[Node<Expr>]) = match &body.kind {
        Expr::Block(exprs) if !exprs.is_empty() => {
            (exprs.last().unwrap(), &exprs[..exprs.len() - 1])
        }
        _ => (body, &[]),
    };
    if matches!(tail.kind, Expr::Lambda { .. } | Expr::Select { .. } | Expr::CoMatch { .. }) {
        (Some(tail), rest.iter().collect())
    } else {
        (None, std::iter::once(body).collect())
    }
}

struct Ctx<'a> {
    /// Operation name → the effect it belongs to.
    op_effect: &'a HashMap<String, String>,
    /// Global function/command → its interface.
    interfaces: &'a HashMap<String, Interface>,
    /// The parameters of the declaration under analysis, name → its row.
    params: &'a HashMap<String, Row>,
    /// Menu/form declarations carrying a latent row.
    latent_decls: &'a HashMap<String, Row>,
    /// Item or field name → its rowed menu/form, for charging demands.
    latent_items: &'a HashMap<String, String>,
    /// `let`-bound names whose value carries a latent row — a consumer
    /// built by a call and bound before being fed. Scoped by hand in the
    /// `Let` arm.
    locals: HashMap<String, Row>,
    diags: &'a mut Vec<Diagnostic>,
}

/// The name a written type is headed by, seen through the signs.
fn type_head(ty: &TypeExpr) -> Option<&str> {
    match ty {
        TypeExpr::Base(name) => Some(name),
        TypeExpr::Apply(name, _) => Some(name),
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) => type_head(&inner.kind),
        _ => None,
    }
}

pub fn check_effects(p: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut op_effect: HashMap<String, String> = HashMap::new();
    for d in &p.decls {
        if let Decl::Effect { name, operations, .. } = &d.kind {
            for op in operations {
                op_effect.insert(op.name.clone(), name.clone());
            }
        }
    }

    // The latent rows of the negative declarations: a menu's row fires per
    // demand, a form's per feed — codata runs on the consumer's schedule,
    // so the row belongs to the type. v1 keeps declaration rows concrete;
    // a row variable on a type is the rows-into-types upgrade, deferred.
    let mut diags = Vec::new();
    let mut latent_decls: HashMap<String, Row> = HashMap::new();
    let mut latent_items: HashMap<String, String> = HashMap::new();
    for d in &p.decls {
        let (name, effects, members) = match &d.kind {
            Decl::Menu { name, effects, items, .. } => (name, effects, items),
            Decl::Form { name, effects, fields, .. } => (name, effects, fields),
            _ => continue,
        };
        if effects.is_empty() {
            continue;
        }
        if !effects.tails.is_empty() {
            diags.push(Diagnostic {
                message: format!(
                    "`{name}` declares a row variable; a declaration's latent row is concrete (row variables on types are not yet supported)"
                ),
                span: d.span,
            });
        }
        for (item, _) in members {
            if let Some(other) = latent_items.insert(item.clone(), name.clone())
                && other != *name
            {
                diags.push(Diagnostic {
                    message: format!(
                        "item `{item}` appears in both `{other}` and `{name}`, which carry latent rows; their item names must be distinct so a demand's row is unambiguous"
                    ),
                    span: d.span,
                });
            }
        }
        latent_decls.insert(name.clone(), Row::from_ast(effects));
    }

    let mut decls = Vec::new();
    let mut interfaces: HashMap<String, Interface> = HashMap::new();
    for d in &p.decls {
        let (name, params, effects, return_type, body) = match &d.kind {
            Decl::Fn { name, params, effects, return_type, body, .. } => {
                (name, params.iter().collect::<Vec<_>>(), effects, return_type, body)
            }
            Decl::Command {
                name,
                value_params,
                continuation_params,
                effects,
                return_type,
                body,
                ..
            } => (
                name,
                value_params.iter().chain(continuation_params).collect(),
                effects,
                return_type,
                body,
            ),
            _ => continue,
        };
        let params: Vec<(String, Row)> = params
            .iter()
            .filter_map(|p| Some(p.name()?.to_string()))
            .zip(params.iter().map(|p| p.ty.as_ref().map(param_row).unwrap_or_default()))
            .collect();
        let latent = return_type.as_ref().map(return_latent).unwrap_or_default();
        interfaces.insert(name.clone(), Interface { row: Row::from_ast(effects), params, latent });
        decls.push((name.clone(), body, d.span));
    }

    for (name, body, span) in &decls {
        let interface = &interfaces[name.as_str()];
        let param_rows: HashMap<String, Row> = interface.params.iter().cloned().collect();
        let mut incurred = Row::default();
        let mut ctx = Ctx {
            op_effect: &op_effect,
            interfaces: &interfaces,
            params: &param_rows,
            latent_decls: &latent_decls,
            latent_items: &latent_items,
            locals: HashMap::new(),
            diags: &mut diags,
        };
        // A declaration whose result carries a latent row may end in the
        // suspended value itself — a returned literal's effects belong to
        // that row, not to the call.
        let (tail, rest) = returned_literal(body, &interface.latent);
        for e in rest {
            collect(e, &mut ctx, &mut incurred);
        }
        if let Some(tail) = tail {
            let mut lit = Row::default();
            collect(tail, &mut ctx, &mut lit);
            for effect in lit.effects.difference(&interface.latent.effects) {
                ctx.diags.push(Diagnostic {
                    message: format!(
                        "the value `{name}` returns performs `{effect}` when run, beyond its declared latent row"
                    ),
                    span: tail.span,
                });
            }
            for t in lit.tails.difference(&interface.latent.tails) {
                ctx.diags.push(Diagnostic {
                    message: format!(
                        "the value `{name}` returns performs the row `..{t}` when run; declare it latent on the result type"
                    ),
                    span: tail.span,
                });
            }
        }

        let allowed = &interface.row;
        for effect in &incurred.effects {
            if !allowed.effects.contains(effect) {
                ctx.diags.push(Diagnostic {
                    message: format!(
                        "`{name}` performs `{effect}` but does not declare it; add `/ {{{effect}}}` to its type, or handle it"
                    ),
                    span: *span,
                });
            }
        }
        for tail in &incurred.tails {
            if !allowed.tails.contains(tail) {
                ctx.diags.push(Diagnostic {
                    message: format!(
                        "`{name}` performs the row `..{tail}` of a parameter but does not declare it; add `..{tail}` to its row"
                    ),
                    span: *span,
                });
            }
        }
        // `main` is the root, and the runtime is its handler — but the
        // runtime handles exactly one effect, so `{IO}` is what may reach it
        // and everything else is handled before.
        if name == "main" {
            let unhandled: Vec<&String> =
                allowed.effects.iter().filter(|e| e.as_str() != IO).collect();
            if !unhandled.is_empty() || !allowed.tails.is_empty() {
                ctx.diags.push(Diagnostic {
                    message: "`main` is the root: the runtime handles `IO`, so its row is \
                              `{IO}` or empty and every other effect is handled before it"
                        .into(),
                    span: *span,
                });
            }
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

/// What a call to `name` charges: an operation performs its effect, a
/// parameter its declared row, a global its own — with each row variable of
/// the callee instantiated from the argument standing where it is mentioned.
/// A flow stage is a call, so it charges through here too.
fn charge_call(name: &str, args: &[Node<Expr>], ctx: &mut Ctx, out: &mut Row) {
    let name = &name.to_string();
    if let Some(effect) = builtin_effect(name) {
        out.effects.insert(effect.to_string());
        return;
    }
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
}

/// The builtins that reach outside the program. Each performs an operation
/// of `IO` — `println` performs `write_line`; the file primitives beneath `fs` reach out directly —
/// so calling one charges `{IO}` exactly as a written operation would.
pub(crate) fn builtin_effect(name: &str) -> Option<&'static str> {
    matches!(
        name,
        "println"
            | "print"
            | "__read_file"
            | "__write_file"
            | "__open_file"
            | "__read_line"
            | "__close_file"
            | "__file_exists"
    )
    .then_some(IO)
}

/// The one effect the runtime itself handles: `main` may leave it
/// undischarged, and nothing else may.
pub(crate) const IO: &str = "IO";

/// The effects an expression may incur, gathered into `out`.
fn collect(e: &Node<Expr>, ctx: &mut Ctx, out: &mut Row) {
    match &e.kind {
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = &callee.kind {
                charge_call(name, args, ctx, out);
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
        // A `mu` over a menu with a latent row: the arms' effects belong to
        // the menu's row — they run per demand, on the demander's schedule —
        // so they are checked against it here and charged to no one.
        Expr::CoMatch { ty: Some(ty), arms }
            if type_head(&ty.kind).and_then(|n| ctx.latent_decls.get(n)).is_some() =>
        {
            let menu = type_head(&ty.kind).unwrap().to_string();
            let allowed = ctx.latent_decls[&menu].clone();
            check_arms_against_latent(arms, &menu, &allowed, ctx, e.span);
        }
        // A `select` over a form with a latent row, likewise: the arms run
        // when the form is fed.
        Expr::Select { ty: Some(ty), arms }
            if type_head(&ty.kind).and_then(|n| ctx.latent_decls.get(n)).is_some() =>
        {
            let form = type_head(&ty.kind).unwrap().to_string();
            let allowed = ctx.latent_decls[&form].clone();
            check_arms_against_latent(arms, &form, &allowed, ctx, e.span);
        }
        // A demand on a rowed menu incurs the menu's latent row: the work
        // happens now, in this dynamic extent.
        Expr::Project { base, key } => {
            if let slc_syntax::ast::ProjKey::Field(field) = key
                && let Some(decl) = ctx.latent_items.get(field)
            {
                out.extend(&ctx.latent_decls[decl].clone());
            }
            collect(base, ctx, out);
        }
        // A flow that closes is a cut, so it charges like one: its first
        // stage is what flows in, its last is what consumes.
        Expr::Flow { stages, into_consumer, .. } => {
            if *into_consumer && let (Some(value), Some(consumer)) = (stages.first(), stages.last())
            {
                charge_cut(value, consumer, ctx, out);
            }
            // A stage is a call: `x | f` *is* `f(x)`, so it charges what the
            // call charges. Only the first stage's argument is syntax — the
            // rest receive what the stage before them produced — so that is
            // the one whose row variables can be instantiated. The closing
            // consumer is not applied and charges nothing of its own.
            let applied = stages.len() - usize::from(*into_consumer);
            for (index, stage) in stages.iter().enumerate().take(applied).skip(1) {
                if let Expr::Ident(name) = &stage.kind {
                    let args: &[Node<Expr>] = match (index, &stages[index - 1].kind) {
                        (1, Expr::Pair(items)) => items,
                        (1, _) => std::slice::from_ref(&stages[0]),
                        _ => &[],
                    };
                    charge_call(name, args, ctx, out);
                }
            }
            for stage in stages {
                collect(stage, ctx, out);
            }
        }
        // A `let` remembers the latent row of what it binds, so a consumer
        // built by a call and fed later is still charged at its cut.
        Expr::Let { pattern, value, body, .. } => {
            collect(value, ctx, out);
            // A block-level `let` has no body of its own — its siblings
            // follow it — so the binding stays for the rest of the walk. A
            // destructuring binder names parts of the value, not the value,
            // so nothing it binds carries the whole thing's latent row.
            if let Some(name) = pattern.binder_name() {
                let latent = latent_of_value(value, ctx);
                if latent.effects.is_empty() && latent.tails.is_empty() {
                    ctx.locals.remove(name);
                } else {
                    ctx.locals.insert(name.to_string(), latent);
                }
            }
            if let Some(body) = body {
                collect(body, ctx, out);
            }
        }
        other => {
            for child in other.children() {
                collect(child, ctx, out);
            }
        }
    }
}

/// What a cut charges: feeding a rowed form incurs its row, and a consumer
/// built by a call incurs the callee's latent result row, its variables
/// instantiated from the call.
fn charge_cut(value: &Node<Expr>, consumer: &Node<Expr>, ctx: &mut Ctx, out: &mut Row) {
    if let Expr::Data { name, .. } = &value.kind
        && let Some(row) = ctx.latent_decls.get(name)
    {
        out.extend(&row.clone());
    }
    match &consumer.kind {
        Expr::Ident(k) => {
            if let Some(row) = ctx.locals.get(k) {
                out.extend(&row.clone());
            } else if let Some(row) = ctx.params.get(k) {
                // A parameter whose type carries a latent row: the
                // cut is where it fires. (A call-row parameter's
                // effects fire at its call instead; the two do not
                // overlap, since one wraps an arrow and the other a
                // consumer.)
                out.extend(&row.clone());
            }
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(g) = &callee.kind
                && let Some(interface) = ctx.interfaces.get(g)
            {
                let latent = interface.latent.clone();
                out.effects.extend(latent.effects.iter().cloned());
                for tail in &latent.tails {
                    for position in ctx.interfaces[g.as_str()].positions_of(tail) {
                        if let Some(arg) = args.get(position)
                            && let Expr::Ident(passed) = &arg.kind
                        {
                            let declared = &ctx.interfaces[g.as_str()].params[position].1;
                            let mut arg_row = row_of_name(passed, ctx);
                            for effect in &declared.effects {
                                arg_row.effects.remove(effect);
                            }
                            out.extend(&arg_row);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// The latent row carried by the value of an expression, as far as names
/// can see: a call's declared result latency (variables instantiated from
/// its arguments), or a rowed `select` literal.
fn latent_of_value(e: &Node<Expr>, ctx: &Ctx) -> Row {
    match &e.kind {
        Expr::Call { callee, args } => {
            let Expr::Ident(g) = &callee.kind else { return Row::default() };
            let Some(interface) = ctx.interfaces.get(g) else { return Row::default() };
            let mut row = Row { effects: interface.latent.effects.clone(), ..Row::default() };
            for tail in &interface.latent.tails {
                for position in interface.positions_of(tail) {
                    if let Some(arg) = args.get(position)
                        && let Expr::Ident(passed) = &arg.kind
                    {
                        let declared = &interface.params[position].1;
                        let mut arg_row = row_of_name(passed, ctx);
                        for effect in &declared.effects {
                            arg_row.effects.remove(effect);
                        }
                        row.extend(&arg_row);
                    }
                }
            }
            row
        }
        Expr::Select { ty: Some(ty), .. } => {
            type_head(&ty.kind).and_then(|n| ctx.latent_decls.get(n)).cloned().unwrap_or_default()
        }
        // A handler discharges only what fires inside it; a latent row has
        // not fired yet, so it passes through the `return` clause.
        Expr::Handle { body, .. } => latent_of_value(body, ctx),
        Expr::Block(exprs) => exprs.last().map(|e| latent_of_value(e, ctx)).unwrap_or_default(),
        _ => Row::default(),
    }
}

/// Check the arms of a suspended literal over a rowed declaration: each
/// arm's effects must fit the declaration's latent row.
fn check_arms_against_latent(
    arms: &[slc_syntax::ast::SelectArm],
    decl: &str,
    allowed: &Row,
    ctx: &mut Ctx,
    span: slc_syntax::token::Span,
) {
    for arm in arms {
        let mut row = Row::default();
        collect(&arm.command, ctx, &mut row);
        for effect in row.effects.difference(&allowed.effects) {
            ctx.diags.push(Diagnostic {
                message: format!(
                    "this arm performs `{effect}`, which `{decl}` does not declare latent; add it: `menu {decl} / {{{effect}}}`"
                ),
                span,
            });
        }
        for tail in &row.tails {
            ctx.diags.push(Diagnostic {
                message: format!(
                    "this arm performs the row `..{tail}`, but a declaration's latent row is concrete; `{decl}` cannot absorb a row variable"
                ),
                span,
            });
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
                 command main | (exit: -i32) / {{IO}} {{ {main_body}; 0 | exit⟩ }}"
            )
        };
        // A pure argument instantiates E to the empty row.
        assert!(check(&src("println(app(inc, 1))")).is_ok());
        // An effectful one flows into the caller, which must answer for it.
        let diags = check(&src("println(app(risky, 1))")).unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
        // Handled at the call, the row is discharged and `main` stays pure.
        assert!(
            check(&src("let r = handle app(risky, 1) { throw(m) => 0 - 1, return(n) => n };
                 println(r)"))
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
             command main | (exit: -i32) / {{IO}} {{ println(app(risky, 1)); 0 | exit⟩ }}"
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
                 command main | (exit: -i32) / {{IO}} {{
                     let r = handle twice(risky, 8) {{ throw(m) => 0 - 1, return(n) => n }};
                     println(r); 0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn an_operation_passed_as_a_value_carries_its_effect() {
        let diags = check(&format!(
            "{EXN} fn app<E>(f: (+String -> +i64 / {{..E}}), x: +String) -> i64 / {{..E}} {{ f(x) }}
             command main | (exit: -i32) / {{IO}} {{ println(app(throw, \"m\")); 0 | exit⟩ }}"
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
                 command main | (exit: -i32) / {{IO}} {{
                     let r = handle guard(risky, 1) {{ throw(m) => 0 - 1, return(n) => n }};
                     println(r); 0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_flow_stage_charges_what_it_performs() {
        // A stage is a call, so the effect follows it: `x | throw` is
        // charged exactly as `throw(x)` is.
        let diags = check(&format!(
            "{EXN} fn risky(n: i64) -> i64 {{ if n > 0 {{ n }} else {{ \"no\" | throw }} }}
             command main | (exit: -i32) / {{IO}} {{ 0 | exit⟩ }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`risky` performs `Exn`")), "{diags:?}");
        assert!(
            check(&format!(
                "{EXN} fn risky(n: i64) -> i64 / {{Exn}} {{ if n > 0 {{ n }} else {{ \"no\" | throw }} }}
                 command main | (exit: -i32) / {{IO}} {{ 0 | exit⟩ }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn printing_performs_io_and_only_main_may_leave_it() {
        // `println` performs `IO`, so a printing declaration declares it.
        let diags = check(
            "fn shout(m: String) -> Unit { m | println }
             command main | (exit: -i32) / {IO} { 0 | exit⟩ }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`shout` performs `IO`")), "{diags:?}");
        assert!(
            check(
                "fn shout(m: String) -> Unit / {IO} { m | println }
                 command main | (exit: -i32) / {IO} { \"hi\" | shout; 0 | exit⟩ }"
            )
            .is_ok()
        );
    }

    #[test]
    fn main_may_leave_only_io_undischarged() {
        let diags = check(&format!(
            "{EXN} command main | (exit: -i32) / {{Exn}} {{ println(throw(\"no\")); 0 | exit⟩ }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` is the root")), "{diags:?}");
    }

    const FALLIBLE: &str = "menu Fallible / {Exn} { value: i64, doubled: i64 }
         fn checked(n: +i64) -> Fallible {
             mu Fallible {
                 value <= (if n >= 0 { n } else { throw(\"neg\") }) | value⟩,
                 doubled <= (if n >= 0 { n * 2 } else { throw(\"neg\") }) | doubled⟩,
             }
         }\n";

    #[test]
    fn a_rowed_menu_charges_demands_not_the_constructor() {
        // `checked` declares no row: the arms belong to Fallible's latent
        // row. The demand is what incurs it — unhandled, it reaches main.
        let diags = check(&format!(
            "{EXN}{FALLIBLE} command main | (exit: -i32) / {{IO}} {{
                 println(checked(1).value); 0 | exit⟩
             }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
        // Handled around the demand — the honest extent — main is pure.
        assert!(
            check(&format!(
                "{EXN}{FALLIBLE} command main | (exit: -i32) / {{IO}} {{
                     println(handle checked(1).value {{
                         throw(m) => 0 - 1, return(n) => n
                     }});
                     0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_mu_arm_beyond_the_latent_row_is_rejected() {
        let diags = check(&format!(
            "{EXN} effect Log {{ fn log(m: +String) -> unit; }}
             menu Fallible / {{Exn}} {{ value: i64 }}
             fn noisy() -> Fallible {{
                 mu Fallible {{ value: out <= {{ log(\"x\"); 1 }} | out⟩ }}
             }}"
        ))
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("performs `Log`")
                && d.message.contains("`Fallible` does not declare")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_rowed_form_charges_the_feed() {
        let diags = check(&format!(
            "{EXN} form Guarded / {{Exn}} {{ value: i64 }}
             fn guard() -> Guarded {{
                 select Guarded {{ Guarded {{ value }} => throw(\"no\") | EXIT⟩ }}
             }}
             command main | (exit: -i32) / {{IO}} {{
                 Guarded {{ value: 1 }} | guard()⟩
             }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn a_latent_result_row_fires_at_the_cut_and_survives_a_handle() {
        // `after` performs nothing when called: its row lives on the
        // returned consumer. The cut is where it fires — and a handler
        // around the CALL discharges nothing, because nothing fired.
        let after = "fn after<E>(f: (+i64 -> +i64 / {..E}), k: -i64) -> (-i64 / {..E}) {
                 fn(x: +i64) { f(x) | k⟩ }
             }
             fn risky(x: +i64) -> i64 / {Exn} { throw(\"late\") }\n";
        let diags = check(&format!(
            "{EXN}{after} command main | (exit: -i32) / {{IO}} {{
                 let n = mu i64 {{ out <= {{
                     let c = handle after(risky, out) {{
                         throw(m) => 0 - 1, return(x) => x
                     }};
                     5 | c⟩
                 }} }};
                 println(n); 0 | exit⟩
             }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`main` performs `Exn`")), "{diags:?}");
        // Around the cut, it is discharged.
        assert!(
            check(&format!(
                "{EXN}{after} command main | (exit: -i32) / {{IO}} {{
                     let n = handle (mu i64 {{ out <= 5 | after(risky, out)⟩ }}) {{
                         throw(m) => 0 - 1, return(x) => x
                     }};
                     println(n); 0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_returned_literal_beyond_the_latent_row_is_rejected() {
        let diags = check(&format!(
            "{EXN} fn quiet(k: -i64) -> (-i64 / {{}}) {{
                 fn(x: +i64) {{ throw(\"loud\") | k⟩ }}
             }}"
        ));
        // `/ {{}}` parses as the empty row, indistinguishable from none —
        // so the latent row here is empty and the literal is charged to
        // `quiet` itself, which declares nothing.
        let diags = match diags {
            Err(d) => d,
            Ok(()) => panic!("a throwing returned literal must be rejected somewhere"),
        };
        assert!(diags.iter().any(|d| d.message.contains("performs `Exn`")), "{diags:?}");
    }

    #[test]
    fn declaration_rows_are_concrete_and_items_unambiguous() {
        let diags = check(
            "effect Exn { fn throw(m: +String) -> i64; }
             menu Bad<E> / {..E} { value: i64 }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("latent row is concrete")), "{diags:?}");

        let diags = check(
            "effect Exn { fn throw(m: +String) -> i64; }
             menu A / {Exn} { value: i64 }
             menu B / {Exn} { value: i64 }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must be distinct")), "{diags:?}");
    }

    #[test]
    fn clause_binders_are_copattern_shaped() {
        // Omitted for a clause that never resumes; bound after a colon,
        // under any name, for one that does.
        assert!(
            check(&format!(
                "{EXN} command main | (exit: -i32) / {{IO}} {{
                     let r = handle throw(\"x\") {{ throw(m) => 0 - 1, return(n) => n }};
                     println(r); 0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
        assert!(
            check(&format!(
                "{EXN} command main | (exit: -i32) / {{IO}} {{
                     let r = handle throw(\"x\") {{ throw(m): k => k(9), return(n) => n }};
                     println(r); 0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn a_negative_function_carries_an_effect_row() {
        // The row sits after the `<-` arrow and is enforced like any other.
        assert!(
            check(
                "effect Log { fn log(m: +String) -> unit; }
                 fn emit(out: -i64) <- i64 / {Log} { log(\"x\"); 42 | out⟩ }"
            )
            .is_ok()
        );
        let diags = check(
            "effect Log { fn log(m: +String) -> unit; }
             fn emit(out: -i64) <- i64 { log(\"x\"); 42 | out⟩ }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`emit` performs `Log`")), "{diags:?}");
    }
}
