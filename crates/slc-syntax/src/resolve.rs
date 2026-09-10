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
    let mut errors = Vec::new();
    let mut out = Vec::new();
    let root = collect_scope(&program.decls, Vec::new(), &mut errors);
    let mut stack = vec![root];
    flatten(&program.decls, &mut stack, &mut out, &mut errors);
    if errors.is_empty() { Ok(Program { decls: out }) } else { Err(errors) }
}

fn collect_scope(decls: &[Node<Decl>], path: Vec<String>, errors: &mut Vec<ResolveError>) -> Scope {
    let mut scope = Scope { path, declares: HashSet::new(), aliases: HashMap::new() };
    for d in decls {
        match &d.kind {
            Decl::Fn { name, .. }
            | Decl::Command { name, .. }
            | Decl::Struct { name, .. }
            | Decl::Enum { name, .. }
            | Decl::Const { name, .. }
            | Decl::Mod { name, .. }
            | Decl::Trait { name, .. } => {
                scope.declares.insert(name.clone());
            }
            Decl::Impl { .. } => {}
            Decl::Use { path } => {
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
            Decl::Use { .. } => {}
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
        Decl::Struct { name, fields, .. } => {
            *name = scope.qualify(name);
            for (_, ty) in fields {
                resolve_type(ty, stack);
            }
        }
        Decl::Enum { name, variants } => {
            *name = scope.qualify(name);
            for (_, payloads) in variants {
                for ty in payloads {
                    resolve_type(ty, stack);
                }
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
        Decl::Impl { trait_name, for_type, methods } => {
            *trait_name = resolve_name(trait_name, stack);
            resolve_type(for_type, stack);
            for method in methods {
                resolve_decl(&mut method.kind, stack, locals);
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
        TypeExpr::Base(name) => *name = resolve_name(name, stack),
        TypeExpr::Positive(inner)
        | TypeExpr::Negative(inner)
        | TypeExpr::List(inner)
        | TypeExpr::Dual(inner)
        | TypeExpr::Down(inner)
        | TypeExpr::Up(inner) => resolve_type(&mut inner.kind, stack),
        TypeExpr::Tensor(a, b) | TypeExpr::Par(a, b) | TypeExpr::Fun(a, b) => {
            resolve_type(&mut a.kind, stack);
            resolve_type(&mut b.kind, stack);
        }
        TypeExpr::Unit | TypeExpr::Bottom => {}
    }
}

fn is_local(name: &str, locals: &[HashSet<String>]) -> bool {
    let first = name.split("::").next().unwrap_or(name);
    locals.iter().any(|frame| frame.contains(first))
}

fn resolve_expr(e: &mut Expr, stack: &[Scope], locals: &mut Vec<HashSet<String>>) {
    match e {
        Expr::Ident(name) => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
        }
        Expr::Struct { name, fields } => {
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
        Expr::Pair(items) => {
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
        Expr::UnOp { body, .. } | Expr::Shift { expr: body, .. } => {
            resolve_expr(&mut body.kind, stack, locals);
        }
        Expr::Cut { value, consumer } => {
            resolve_expr(&mut value.kind, stack, locals);
            resolve_expr(&mut consumer.kind, stack, locals);
        }
        Expr::ErrorProp { expr, .. } => resolve_expr(&mut expr.kind, stack, locals),
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
        Pattern::Enum { name, fields, .. } => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
            for field in fields {
                resolve_pattern(field, stack, locals);
            }
        }
        Pattern::Struct { name, fields } => {
            if !is_local(name, locals) {
                *name = resolve_name(name, stack);
            }
            for (_, field) in fields {
                resolve_pattern(field, stack, locals);
            }
        }
        Pattern::Or(items) | Pattern::Tuple(items) => {
            for item in items {
                resolve_pattern(item, stack, locals);
            }
        }
        Pattern::Binding { pattern, .. } => resolve_pattern(pattern, stack, locals),
        Pattern::Range { start, end } => {
            resolve_pattern(start, stack, locals);
            resolve_pattern(end, stack, locals);
        }
        Pattern::List { items, rest } => {
            for item in items {
                resolve_pattern(item, stack, locals);
            }
            if let Some(rest) = rest {
                resolve_pattern(rest, stack, locals);
            }
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
        Pattern::Binding { name, pattern } => {
            out.insert(name.clone());
            collect_binders(pattern, out);
        }
        Pattern::Or(items) | Pattern::Tuple(items) => {
            for item in items {
                collect_binders(item, out);
            }
        }
        Pattern::Enum { fields, .. } => {
            for field in fields {
                collect_binders(field, out);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (_, field) in fields {
                collect_binders(field, out);
            }
        }
        Pattern::List { items, rest } => {
            for item in items {
                collect_binders(item, out);
            }
            if let Some(rest) = rest {
                collect_binders(rest, out);
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
