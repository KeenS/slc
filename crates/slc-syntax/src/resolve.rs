//! Name resolution: modules flatten into qualified names.
//!
//! A `mod m { … }` is a scope, not a runtime thing. Resolution rewrites the
//! program so that nothing after it has to know modules existed: every
//! declaration inside `m` is renamed `m::name`, every reference is rewritten
//! to the qualified name it resolves to, and the `mod`/`use` declarations
//! themselves disappear. The checker, the lowering, and the runtime keep
//! working on flat names — which already contain `::`, because enum variants
//! always did.
//!
//! A name resolves in scope order: a local binding shadows everything and is
//! left alone; then the `use` aliases of the enclosing module; then the
//! module's own declarations; then each ancestor module's, out to the root.
//! A path resolves by its first segment and keeps the rest.

use crate::ast::{Decl, Expr, Node, Param, Pattern, Program, TypeExpr};
use crate::token::Span;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolveError {
    pub message: String,
    pub span: Span,
}

/// One module's names: what it declares, and what it `use`s.
struct Scope {
    /// The module's path from the root, `["a", "b"]` for `a::b`.
    path: Vec<String>,
    /// Names this module declares: functions, commands, types, constants,
    /// and child modules.
    declares: HashSet<String>,
    /// `use a::b::c;` makes `c` mean `a::b::c` here.
    aliases: HashMap<String, String>,
}

impl Scope {
    fn qualify(&self, name: &str) -> String {
        if self.path.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.path.join("::"), name)
        }
    }
}

/// Flatten every module, qualifying declarations and references.
pub fn resolve_program(program: &Program) -> Result<Program, Vec<ResolveError>> {
    resolve_program_split(program, usize::MAX)
}

/// Resolve a program whose source is two units — the program's own text,
/// then the prelude appended from `prelude_from` — with variant imports
/// scoped to their unit: the prelude's `use List::*;` pins names in the
/// prelude only, and a program's imports never reach into the prelude.
pub fn resolve_program_split(
    program: &Program,
    prelude_from: usize,
) -> Result<Program, Vec<ResolveError>> {
    let mut errors = Vec::new();
    let mut out = Vec::new();
    let root = collect_scope(&program.decls, Vec::new(), &mut errors);
    let mut stack = vec![root];
    flatten(&program.decls, &mut stack, &mut out, &mut errors);
    let out = apply_variant_imports(out, prelude_from, &mut errors);
    if errors.is_empty() { Ok(Program { decls: out }) } else { Err(errors) }
}

/// Apply `use Enum::*;` and `use Enum::{A, B};`: every bare use of an
/// imported variant — in patterns and in expressions — is rewritten to its
/// qualified label, so the automatic unqualified-while-unambiguous rule
/// never has to guess about it. An import that collides with another, or
/// names a variant its enum does not have, is an error at the `use`.
fn apply_variant_imports(
    decls: Vec<Node<Decl>>,
    prelude_from: usize,
    errors: &mut Vec<ResolveError>,
) -> Vec<Node<Decl>> {
    use crate::ast::UseImports;
    // The variants of every flattened enum (and every menu's items, for
    // symmetry — though destructors are never bare).
    let mut variants_of: HashMap<String, Vec<String>> = HashMap::new();
    for d in &decls {
        if let Decl::Enum { name, variants, .. } = &d.kind {
            variants_of.insert(name.clone(), variants.iter().map(|(v, _)| v.clone()).collect());
        }
    }
    // Bare name → qualified label, one table per source unit: an import is
    // scoped to the unit that wrote it. A second import of the same name in
    // the same unit is an error.
    let mut tables: [HashMap<String, String>; 2] = [HashMap::new(), HashMap::new()];
    let unit = |span_start: usize| usize::from(span_start >= prelude_from);
    let mut keep = Vec::new();
    for d in decls {
        let Decl::Use { path, imports } = &d.kind else {
            keep.push(d);
            continue;
        };
        let imported = &mut tables[unit(d.span.start)];
        let enum_name = path.join("::");
        let Some(variants) = variants_of.get(&enum_name) else {
            errors.push(ResolveError {
                message: format!("`use {enum_name}::…` does not name a declared enum"),
                span: d.span,
            });
            continue;
        };
        let names: Vec<String> = match imports {
            UseImports::Glob => variants.clone(),
            UseImports::Names(names) => names.clone(),
            UseImports::Member => continue,
        };
        for name in names {
            if !variants.contains(&name) {
                errors.push(ResolveError {
                    message: format!("`{enum_name}` has no variant `{name}`"),
                    span: d.span,
                });
                continue;
            }
            let label = format!("{enum_name}::{name}");
            if let Some(previous) = imported.insert(name.clone(), label.clone())
                && previous != label
            {
                errors.push(ResolveError {
                    message: format!("`{name}` is imported from both `{previous}` and `{label}`"),
                    span: d.span,
                });
            }
        }
    }
    for d in &mut keep {
        let imported = &tables[unit(d.span.start)];
        if !imported.is_empty() {
            rewrite_decl_imports(&mut d.kind, imported);
        }
    }
    keep
}

fn collect_scope(decls: &[Node<Decl>], path: Vec<String>, errors: &mut Vec<ResolveError>) -> Scope {
    let mut scope = Scope { path, declares: HashSet::new(), aliases: HashMap::new() };
    for d in decls {
        match &d.kind {
            Decl::Fn { name, .. }
            | Decl::Command { name, .. }
            | Decl::Data { name, .. }
            | Decl::Enum { name, .. }
            | Decl::Menu { name, .. }
            | Decl::Form { name, .. }
            | Decl::Const { name, .. }
            | Decl::Mod { name, .. }
            | Decl::Trait { name, .. } => {
                scope.declares.insert(name.clone());
            }
            Decl::Effect { name, operations } => {
                scope.declares.insert(name.clone());
                for op in operations {
                    scope.declares.insert(op.name.clone());
                }
            }
            Decl::Impl { .. } => {}
            Decl::Use { path, imports } => {
                if !matches!(imports, crate::ast::UseImports::Member) {
                    continue;
                }
                let target = path.join("::");
                let name = path.last().expect("a use path has segments").clone();
                if scope.aliases.insert(name.clone(), target).is_some() {
                    errors.push(ResolveError {
                        message: format!("`{name}` is brought in by two `use` declarations"),
                        span: d.span,
                    });
                }
            }
        }
    }
    scope
}

fn flatten(
    decls: &[Node<Decl>],
    stack: &mut Vec<Scope>,
    out: &mut Vec<Node<Decl>>,
    errors: &mut Vec<ResolveError>,
) {
    for d in decls {
        match &d.kind {
            Decl::Mod { name, decls } => {
                let mut path = stack.last().expect("a scope").path.clone();
                path.push(name.clone());
                let scope = collect_scope(decls, path, errors);
                stack.push(scope);
                flatten(decls, stack, out, errors);
                stack.pop();
            }
            Decl::Use { path, imports } => {
                // A variant import survives flattening — with the enum it
                // names resolved to its flat name — and a later pass applies
                // it to the whole program.
                if !matches!(imports, crate::ast::UseImports::Member) {
                    let mut path = path.clone();
                    if let Some(first) = path.first_mut() {
                        *first = resolve_name(first, stack);
                    }
                    out.push(Node {
                        span: d.span,
                        kind: Decl::Use { path, imports: imports.clone() },
                    });
                }
            }
            other => {
                let mut resolved = other.clone();
                let locals = &mut Vec::new();
                resolve_decl(&mut resolved, stack, locals);
                out.push(Node { span: d.span, kind: resolved });
            }
        }
    }
}

/// The qualified name `written` resolves to, or `written` itself when
/// nothing in scope claims it — a builtin, a base type, a variant name that
/// the checker resolves through its own table.
fn resolve_name(written: &str, stack: &[Scope]) -> String {
    let (first, rest) = match written.split_once("::") {
        Some((first, rest)) => (first, Some(rest)),
        None => (written, None),
    };
    for scope in stack.iter().rev() {
        if let Some(target) = scope.aliases.get(first) {
            return match rest {
                Some(rest) => format!("{target}::{rest}"),
                None => target.clone(),
            };
        }
        if scope.declares.contains(first) {
            let qualified = scope.qualify(first);
            return match rest {
                Some(rest) => format!("{qualified}::{rest}"),
                None => qualified,
            };
        }
    }
    written.to_string()
}

fn resolve_decl(d: &mut Decl, stack: &[Scope], locals: &mut Vec<HashSet<String>>) {
    let scope = stack.last().expect("a scope");
    match d {
        Decl::Fn { name, params, return_type, body, .. } => {
            *name = scope.qualify(name);
            let mut bound = HashSet::new();
            for p in params.iter_mut() {
                resolve_param(p, stack);
                bound.insert(p.name.clone());
            }
            if let Some(ty) = return_type {
                resolve_type(ty, stack);
            }
            locals.push(bound);
            resolve_expr(&mut body.kind, stack, locals);
            locals.pop();
        }
        Decl::Command { name, value_params, continuation_params, return_type, body, .. } => {
            *name = scope.qualify(name);
            let mut bound = HashSet::new();
            for p in value_params.iter_mut().chain(continuation_params.iter_mut()) {
                resolve_param(p, stack);
                bound.insert(p.name.clone());
            }
            if let Some(ty) = return_type {
                resolve_type(ty, stack);
            }
            locals.push(bound);
            resolve_expr(&mut body.kind, stack, locals);
            locals.pop();
        }
        Decl::Data { name, fields, .. } => {
            *name = scope.qualify(name);
            for (_, ty) in fields {
                resolve_type(ty, stack);
            }
        }
        Decl::Enum { name, variants, .. } => {
            *name = scope.qualify(name);
            for (_, payloads) in variants {
                for ty in payloads {
                    resolve_type(ty, stack);
                }
            }
        }
        Decl::Menu { name, items, .. } => {
            *name = scope.qualify(name);
            for (_, ty) in items {
                resolve_type(ty, stack);
            }
        }
        Decl::Form { name, fields, .. } => {
            *name = scope.qualify(name);
            for (_, ty) in fields {
                resolve_type(ty, stack);
            }
        }
        Decl::Const { name, ty, value } => {
            *name = scope.qualify(name);
            resolve_type(ty, stack);
            resolve_expr(&mut value.kind, stack, locals);
        }
        Decl::Trait { name, methods } => {
            *name = scope.qualify(name);
            for m in methods {
                for p in m.value_params.iter_mut().chain(m.continuation_params.iter_mut()) {
                    resolve_param(p, stack);
                }
                if let Some(ty) = &mut m.return_type {
                    resolve_type(ty, stack);
                }
            }
        }
        Decl::Impl { trait_name, for_type, methods, .. } => {
            *trait_name = resolve_name(trait_name, stack);
            resolve_type(for_type, stack);
            for method in methods {
                resolve_decl(&mut method.kind, stack, locals);
            }
        }
        Decl::Effect { name, operations } => {
            *name = scope.qualify(name);
            for op in operations {
                for p in op.params.iter_mut() {
                    resolve_param(p, stack);
                }
                if let Some(ty) = &mut op.return_type {
                    resolve_type(ty, stack);
                }
            }
        }
        Decl::Mod { .. } | Decl::Use { .. } => {}
    }
}

fn resolve_param(p: &mut Param, stack: &[Scope]) {
    if let Some(ty) = &mut p.ty {
        resolve_type(ty, stack);
    }
}

fn resolve_type(ty: &mut TypeExpr, stack: &[Scope]) {
    match ty {
        TypeExpr::Apply(name, args) => {
            *name = resolve_name(name, stack);
            for arg in args {
                resolve_type(&mut arg.kind, stack);
            }
        }
        TypeExpr::Base(name) => *name = resolve_name(name, stack),
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) | TypeExpr::Dual(inner) => {
            resolve_type(&mut inner.kind, stack)
        }
        TypeExpr::Tensor(a, b)
        | TypeExpr::Par(a, b)
        | TypeExpr::With(a, b)
        | TypeExpr::Fun(a, b) => {
            resolve_type(&mut a.kind, stack);
            resolve_type(&mut b.kind, stack);
        }
        // The row names effects, not types; only the arrow resolves.
        TypeExpr::Effectful(inner, _) => resolve_type(&mut inner.kind, stack),
        TypeExpr::Unit | TypeExpr::Bottom => {}
    }
}

fn is_local(name: &str, locals: &[HashSet<String>]) -> bool {
    let first = name.split("::").next().unwrap_or(name);
    locals.iter().any(|frame| frame.contains(first))
}

fn resolve_expr(e: &mut Expr, stack: &[Scope], locals: &mut Vec<HashSet<String>>) {
    match e {
        Expr::Request { arg, .. } => {
            resolve_expr(&mut arg.kind, stack, locals);
        }
        Expr::Ident(name) => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
        }
        Expr::Data { name, fields } => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
            for (_, value) in fields {
                resolve_expr(&mut value.kind, stack, locals);
            }
        }
        Expr::Lambda { param, param_type, return_type, body } => {
            if let Some(ty) = param_type {
                resolve_type(ty, stack);
            }
            if let Some(ty) = return_type {
                resolve_type(ty, stack);
            }
            locals.push(HashSet::from([param.clone()]));
            resolve_expr(&mut body.kind, stack, locals);
            locals.pop();
        }
        Expr::Mu { continuation_params, body, .. } => {
            let mut bound = HashSet::new();
            for p in continuation_params.iter_mut() {
                resolve_param(p, stack);
                bound.insert(p.name.clone());
            }
            locals.push(bound);
            resolve_expr(&mut body.kind, stack, locals);
            locals.pop();
        }
        Expr::Let { name, ty, value, body } => {
            if let Some(ty) = ty {
                resolve_type(ty, stack);
            }
            resolve_expr(&mut value.kind, stack, locals);
            locals.push(HashSet::from([name.clone()]));
            if let Some(body) = body {
                resolve_expr(&mut body.kind, stack, locals);
            }
            locals.pop();
        }
        Expr::Block(items) => {
            // A `let` in a block scopes over the rest of it.
            let mut opened = 0;
            for item in items.iter_mut() {
                resolve_expr(&mut item.kind, stack, locals);
                if let Expr::Let { name, body: None, .. } = &item.kind {
                    locals.push(HashSet::from([name.clone()]));
                    opened += 1;
                }
            }
            for _ in 0..opened {
                locals.pop();
            }
        }
        Expr::Match { scrutinee, arms } => {
            resolve_expr(&mut scrutinee.kind, stack, locals);
            for arm in arms {
                resolve_pattern(&mut arm.pattern, stack, locals);
                locals.push(pattern_binders(&arm.pattern));
                if let Some(guard) = &mut arm.guard {
                    resolve_expr(&mut guard.kind, stack, locals);
                }
                resolve_expr(&mut arm.body.kind, stack, locals);
                locals.pop();
            }
        }
        Expr::CoMatch { ty, arms } => {
            if let Some(ty) = ty {
                resolve_type(&mut ty.kind, stack);
            }
            for arm in arms {
                resolve_pattern(&mut arm.pattern, stack, locals);
                locals.push(pattern_binders(&arm.pattern));
                resolve_expr(&mut arm.command.kind, stack, locals);
                locals.pop();
            }
        }
        Expr::Select { ty, arms } => {
            if let Some(ty) = ty {
                resolve_type(&mut ty.kind, stack);
            }
            for arm in arms {
                resolve_pattern(&mut arm.pattern, stack, locals);
                locals.push(pattern_binders(&arm.pattern));
                resolve_expr(&mut arm.command.kind, stack, locals);
                locals.pop();
            }
        }
        Expr::Call { callee, args } => {
            resolve_expr(&mut callee.kind, stack, locals);
            for arg in args {
                resolve_expr(&mut arg.kind, stack, locals);
            }
        }
        Expr::Pair(items) | Expr::Bundle(items) | Expr::Flow { stages: items, .. } => {
            for item in items {
                resolve_expr(&mut item.kind, stack, locals);
            }
        }
        Expr::If { cond, then, otherwise } => {
            resolve_expr(&mut cond.kind, stack, locals);
            resolve_expr(&mut then.kind, stack, locals);
            if let Some(otherwise) = otherwise {
                resolve_expr(&mut otherwise.kind, stack, locals);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            resolve_expr(&mut lhs.kind, stack, locals);
            resolve_expr(&mut rhs.kind, stack, locals);
        }
        Expr::UnOp { body, .. } | Expr::Project { base: body, .. } => {
            resolve_expr(&mut body.kind, stack, locals);
        }
        Expr::Cut { value, consumer } => {
            resolve_expr(&mut value.kind, stack, locals);
            resolve_expr(&mut consumer.kind, stack, locals);
        }
        Expr::Handle { body, clauses, ret } => {
            resolve_expr(&mut body.kind, stack, locals);
            for c in clauses.iter_mut() {
                let mut bound: HashSet<String> = c.params.iter().cloned().collect();
                bound.insert(c.resume.clone());
                locals.push(bound);
                resolve_expr(&mut c.body.kind, stack, locals);
                locals.pop();
            }
            if let Some((binder, rbody)) = ret {
                locals.push(HashSet::from([binder.clone()]));
                resolve_expr(&mut rbody.kind, stack, locals);
                locals.pop();
            }
        }
        Expr::Index { value, index } => {
            resolve_expr(&mut value.kind, stack, locals);
            resolve_expr(&mut index.kind, stack, locals);
        }
        Expr::Slice { value, start, end } => {
            resolve_expr(&mut value.kind, stack, locals);
            if let Some(start) = start {
                resolve_expr(&mut start.kind, stack, locals);
            }
            if let Some(end) = end {
                resolve_expr(&mut end.kind, stack, locals);
            }
        }
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) | Expr::Bool(_) => {}
    }
}

/// Qualify the type names a pattern writes. A bare `Pattern::Ident` is left
/// alone: it may be a binder or an unqualified variant, and the checker's
/// variant table tells them apart after flattening.
fn resolve_pattern(p: &mut Pattern, stack: &[Scope], locals: &[HashSet<String>]) {
    match p {
        // The destructor is resolved against the menu table after
        // flattening, like an unqualified variant name.
        Pattern::Dtor { .. } => {}
        Pattern::Enum { name, fields, .. } => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
            for field in fields {
                resolve_pattern(field, stack, locals);
            }
        }
        Pattern::Data { name, fields } => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
            for (_, field) in fields {
                resolve_pattern(field, stack, locals);
            }
        }
        Pattern::Or(items) | Pattern::Tuple(items) | Pattern::Bundle(items) => {
            for item in items {
                resolve_pattern(item, stack, locals);
            }
        }
        Pattern::Binding { pattern, .. } => resolve_pattern(pattern, stack, locals),
        Pattern::Range { start, end } => {
            resolve_pattern(start, stack, locals);
            resolve_pattern(end, stack, locals);
        }
        Pattern::Ident(_)
        | Pattern::Wildcard
        | Pattern::Int(_)
        | Pattern::Str(_)
        | Pattern::Char(_)
        | Pattern::Bool(_)
        | Pattern::Float(_)
        | Pattern::Rest => {}
    }
}

/// Every name a pattern binds, shadowing outer scopes over its arm.
fn pattern_binders(p: &Pattern) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_binders(p, &mut out);
    out
}

fn collect_binders(p: &Pattern, out: &mut HashSet<String>) {
    match p {
        Pattern::Ident(name) => {
            out.insert(name.clone());
        }
        Pattern::Dtor { arg, .. } => {
            collect_binders(arg, out);
        }
        Pattern::Binding { name, pattern } => {
            out.insert(name.clone());
            collect_binders(pattern, out);
        }
        Pattern::Or(items) | Pattern::Tuple(items) | Pattern::Bundle(items) => {
            for item in items {
                collect_binders(item, out);
            }
        }
        Pattern::Enum { fields, .. } => {
            for field in fields {
                collect_binders(field, out);
            }
        }
        Pattern::Data { fields, .. } => {
            for (_, field) in fields {
                collect_binders(field, out);
            }
        }
        Pattern::Range { .. }
        | Pattern::Wildcard
        | Pattern::Int(_)
        | Pattern::Str(_)
        | Pattern::Char(_)
        | Pattern::Bool(_)
        | Pattern::Float(_)
        | Pattern::Rest => {}
    }
}

/// Rewrite every bare use of an imported variant to its qualified label —
/// in expressions and in patterns, through every declaration body. Imported
/// names take precedence over like-named locals, as a variant does in a
/// pattern.
fn rewrite_decl_imports(d: &mut Decl, imported: &HashMap<String, String>) {
    match d {
        Decl::Fn { body, .. } => rewrite_expr_imports(&mut body.kind, imported),
        Decl::Command { body, .. } => rewrite_expr_imports(&mut body.kind, imported),
        Decl::Const { value, .. } => rewrite_expr_imports(&mut value.kind, imported),
        Decl::Impl { methods, .. } => {
            for method in methods {
                rewrite_decl_imports(&mut method.kind, imported);
            }
        }
        _ => {}
    }
}

fn rewrite_expr_imports(e: &mut Expr, imported: &HashMap<String, String>) {
    match e {
        Expr::Ident(name) => {
            if let Some(label) = imported.get(name) {
                *name = label.clone();
            }
        }
        Expr::Match { scrutinee, arms } => {
            rewrite_expr_imports(&mut scrutinee.kind, imported);
            for arm in arms {
                rewrite_pattern_imports(&mut arm.pattern, imported);
                if let Some(guard) = &mut arm.guard {
                    rewrite_expr_imports(&mut guard.kind, imported);
                }
                rewrite_expr_imports(&mut arm.body.kind, imported);
            }
        }
        Expr::Select { arms, .. } | Expr::CoMatch { arms, .. } => {
            for arm in arms {
                rewrite_pattern_imports(&mut arm.pattern, imported);
                rewrite_expr_imports(&mut arm.command.kind, imported);
            }
        }
        Expr::Lambda { body, .. } | Expr::UnOp { body, .. } | Expr::Mu { body, .. } => {
            rewrite_expr_imports(&mut body.kind, imported)
        }
        Expr::Call { callee, args } => {
            rewrite_expr_imports(&mut callee.kind, imported);
            for arg in args {
                rewrite_expr_imports(&mut arg.kind, imported);
            }
        }
        Expr::Pair(items)
        | Expr::Bundle(items)
        | Expr::Flow { stages: items, .. }
        | Expr::Block(items) => {
            for item in items {
                rewrite_expr_imports(&mut item.kind, imported);
            }
        }
        Expr::Data { fields, .. } => {
            for (_, value) in fields {
                rewrite_expr_imports(&mut value.kind, imported);
            }
        }
        Expr::Let { value, body, .. } => {
            rewrite_expr_imports(&mut value.kind, imported);
            if let Some(body) = body {
                rewrite_expr_imports(&mut body.kind, imported);
            }
        }
        Expr::If { cond, then, otherwise } => {
            rewrite_expr_imports(&mut cond.kind, imported);
            rewrite_expr_imports(&mut then.kind, imported);
            if let Some(otherwise) = otherwise {
                rewrite_expr_imports(&mut otherwise.kind, imported);
            }
        }
        Expr::BinOp { lhs, rhs, .. } => {
            rewrite_expr_imports(&mut lhs.kind, imported);
            rewrite_expr_imports(&mut rhs.kind, imported);
        }
        Expr::Cut { value, consumer } => {
            rewrite_expr_imports(&mut value.kind, imported);
            rewrite_expr_imports(&mut consumer.kind, imported);
        }
        Expr::Request { arg: expr, .. } => rewrite_expr_imports(&mut expr.kind, imported),
        Expr::Project { base, .. } => rewrite_expr_imports(&mut base.kind, imported),
        Expr::Handle { body, clauses, ret } => {
            rewrite_expr_imports(&mut body.kind, imported);
            for clause in clauses {
                rewrite_expr_imports(&mut clause.body.kind, imported);
            }
            if let Some((_, ret)) = ret {
                rewrite_expr_imports(&mut ret.kind, imported);
            }
        }
        Expr::Index { value, index } => {
            rewrite_expr_imports(&mut value.kind, imported);
            rewrite_expr_imports(&mut index.kind, imported);
        }
        Expr::Slice { value, start, end } => {
            rewrite_expr_imports(&mut value.kind, imported);
            for endpoint in [start, end].into_iter().flatten() {
                rewrite_expr_imports(&mut endpoint.kind, imported);
            }
        }
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) | Expr::Bool(_) => {}
    }
}

fn rewrite_pattern_imports(p: &mut Pattern, imported: &HashMap<String, String>) {
    match p {
        // A bare imported name is that variant, payloadless.
        Pattern::Ident(name) => {
            if let Some(label) = imported.get(name) {
                let (enum_name, variant) = label.rsplit_once("::").expect("a qualified label");
                *p = Pattern::Enum {
                    name: enum_name.to_string(),
                    variant: variant.to_string(),
                    fields: Vec::new(),
                };
            }
        }
        // `Cons(h, t)` parses with the variant in `name`; qualify it.
        Pattern::Enum { name, variant, fields } => {
            if variant.is_empty()
                && let Some(label) = imported.get(name)
            {
                let (enum_name, v) = label.rsplit_once("::").expect("a qualified label");
                *name = enum_name.to_string();
                *variant = v.to_string();
            }
            for field in fields {
                rewrite_pattern_imports(field, imported);
            }
        }
        Pattern::Binding { pattern, .. } => rewrite_pattern_imports(pattern, imported),
        Pattern::Or(items) | Pattern::Tuple(items) | Pattern::Bundle(items) => {
            for item in items {
                rewrite_pattern_imports(item, imported);
            }
        }
        Pattern::Range { start, end } => {
            rewrite_pattern_imports(start, imported);
            rewrite_pattern_imports(end, imported);
        }
        Pattern::Data { fields, .. } => {
            for (_, field) in fields {
                rewrite_pattern_imports(field, imported);
            }
        }
        Pattern::Dtor { arg, .. } => rewrite_pattern_imports(arg, imported),
        _ => {}
    }
}
