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

use crate::ast::{Decl, EffectRow, Expr, Node, Param, Pattern, Program, TypeExpr};
use crate::token::Span;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolveError {
    pub message: String,
    pub span: Span,
}

/// One module's names: what it declares, and what it `use`s.
struct Scope {
    type_parameters: std::cell::RefCell<Vec<HashSet<String>>>,
    /// The module's path from the root, `["a", "b"]` for `a::b`.
    path: Vec<String>,
    /// Names this module declares: functions, commands, types, constants,
    /// and child modules.
    declares: HashSet<String>,
    /// `use a::b::c;` makes `c` mean `a::b::c` here.
    aliases: HashMap<String, String>,
    /// `use m::*;` — every `pub` member of module `m`, bare, by the targets
    /// each name could mean. Weaker than an explicit `use` and than the
    /// module's own declarations; two globs bringing one name make it
    /// ambiguous, which is an error only where the name is used.
    globs: HashMap<String, Vec<String>>,
    /// Ambiguous glob names met while resolving, kept on the root scope and
    /// drained by `flatten` onto the declaration that used them — resolving
    /// a name has nowhere else to report.
    ambiguous: std::cell::RefCell<Vec<(String, Vec<String>)>>,
}

/// Every module's members, by the module's qualified name, with whether each
/// is `pub` — what `use m::*;` draws from.
type Modules = HashMap<String, Vec<(String, bool)>>;

fn collect_modules(decls: &[Node<Decl>], prefix: &str, out: &mut Modules) {
    for d in decls {
        let Decl::Mod { name, decls: inner, .. } = &d.kind else { continue };
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}::{name}") };
        let mut members = Vec::new();
        for m in inner {
            match &m.kind {
                Decl::Fn { name, is_public, .. }
                | Decl::Command { name, is_public, .. }
                | Decl::Data { name, is_public, .. }
                | Decl::Enum { name, is_public, .. }
                | Decl::Menu { name, is_public, .. }
                | Decl::Form { name, is_public, .. }
                | Decl::Const { name, is_public, .. }
                | Decl::Trait { name, is_public, .. }
                | Decl::Mod { name, is_public, .. } => members.push((name.clone(), *is_public)),
                Decl::Effect { name, is_public, operations, .. } => {
                    members.push((name.clone(), *is_public));
                    for op in operations {
                        members.push((op.name.clone(), *is_public));
                    }
                }
                _ => {}
            }
        }
        out.insert(path.clone(), members);
        collect_modules(inner, &path, out);
    }
}

/// The module a glob's path names, looked up the way a name is: from the
/// importing module outward to the root.
fn find_module(path: &[String], here: &[String], modules: &Modules) -> Option<String> {
    let written = path.join("::");
    (0..=here.len()).rev().find_map(|k| {
        let candidate =
            if k == 0 { written.clone() } else { format!("{}::{written}", here[..k].join("::")) };
        modules.contains_key(&candidate).then_some(candidate)
    })
}

/// Fill a scope's globs from its `use m::*;` declarations that name modules.
/// A glob over an enum is a variant import, handled after flattening.
fn expand_globs(scope: &mut Scope, decls: &[Node<Decl>], modules: &Modules) {
    for d in decls {
        let Decl::Use { path, imports: crate::ast::UseImports::Glob } = &d.kind else { continue };
        let Some(module) = find_module(path, &scope.path, modules) else { continue };
        for (member, is_public) in &modules[&module] {
            if !is_public {
                continue;
            }
            let target = format!("{module}::{member}");
            let candidates = scope.globs.entry(member.clone()).or_default();
            if !candidates.contains(&target) {
                candidates.push(target);
            }
        }
    }
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
    resolve_program_split(program, &[])
}

/// Which source unit a span falls in: the program is unit 0, and each
/// boundary in `units` starts the next. Boundaries are char offsets, as
/// spans are.
fn unit_of(span_start: usize, units: &[usize]) -> usize {
    units.iter().filter(|&&from| span_start >= from).count()
}

/// Resolve a program whose source is several units — the program's own
/// text, then each library unit appended from the offset `units` lists for
/// it — with imports scoped to their unit: a library file's `use Enum::*;`
/// pins names in that file only, and a program's imports never reach into
/// the library.
///
/// The root scope is one scope over every unit, so a top-level `use a::b;`
/// in a library unit would make `b` mean `a::b` in the program too. A
/// library unit therefore imports names only inside its `mod`; at its top
/// level it may import variants — which are scoped per unit — and nothing
/// else.
pub fn resolve_program_split(
    program: &Program,
    units: &[usize],
) -> Result<Program, Vec<ResolveError>> {
    let mut errors = Vec::new();
    let mut modules = Modules::new();
    collect_modules(&program.decls, "", &mut modules);
    for d in &program.decls {
        let brings_names = match &d.kind {
            Decl::Use { imports: crate::ast::UseImports::Member, .. } => true,
            Decl::Use { path, imports: crate::ast::UseImports::Glob } => {
                find_module(path, &[], &modules).is_some()
            }
            _ => false,
        };
        if brings_names && unit_of(d.span.start, units) > 0 {
            errors.push(ResolveError {
                message: "a library unit imports names inside its `mod`, not at the top: \
                          the root scope is shared with the program"
                    .into(),
                span: d.span,
            });
        }
    }
    let mut out = Vec::new();
    let mut root = collect_scope(&program.decls, Vec::new(), &mut errors);
    expand_globs(&mut root, &program.decls, &modules);
    let mut stack = vec![root];
    flatten(&program.decls, &mut stack, &mut out, &modules, &mut errors);
    let out = apply_variant_imports(out, units, &mut errors);
    check_visibility(&out, &mut errors);
    if errors.is_empty() { Ok(Program { decls: out }) } else { Err(errors) }
}

/// Enforce visibility, once every name is qualified.
///
/// A declaration inside a `mod` is private unless it is `pub`: reachable
/// from that module and the modules nested inside it, and nowhere else. A
/// top-level declaration is in no module and is visible everywhere, which
/// is what lets the prelude be the prelude.
///
/// The check runs after flattening because that is where both halves are
/// known: a reference is a qualified name, and the declaration it sits in
/// carries the module it was written in.
fn check_visibility(decls: &[Node<Decl>], errors: &mut Vec<ResolveError>) {
    let mut public: HashMap<String, bool> = HashMap::new();
    for d in decls {
        let (name, is_public) = match &d.kind {
            Decl::Fn { name, is_public, .. }
            | Decl::Command { name, is_public, .. }
            | Decl::Data { name, is_public, .. }
            | Decl::Enum { name, is_public, .. }
            | Decl::Menu { name, is_public, .. }
            | Decl::Form { name, is_public, .. }
            | Decl::Const { name, is_public, .. }
            | Decl::Trait { name, is_public, .. }
            | Decl::Effect { name, is_public, .. } => (name, *is_public),
            _ => continue,
        };
        public.insert(name.clone(), is_public);
        if let Decl::Effect { operations, .. } = &d.kind {
            for op in operations {
                let qualified = match name.rsplit_once("::") {
                    Some((path, _)) => format!("{path}::{}", op.name),
                    None => op.name.clone(),
                };
                public.insert(qualified, is_public);
            }
        }
    }
    for d in decls {
        let Some(owner) = declared_name(&d.kind) else { continue };
        let here = module_of(owner);
        let mut seen = Vec::new();
        references(&d.kind, &mut seen);
        for name in seen {
            // The longest declared prefix is what the reference reaches:
            // `a::Colour::Red` reaches the enum `a::Colour`.
            let mut target = name.as_str();
            let reachable = loop {
                if let Some(is_public) = public.get(target) {
                    break Some((target.to_string(), *is_public));
                }
                match target.rsplit_once("::") {
                    Some((head, _)) => target = head,
                    None => break None,
                }
            };
            let Some((target, is_public)) = reachable else { continue };
            let owning = module_of(&target);
            if is_public || owning.is_empty() || visible_from(&owning, &here) {
                continue;
            }
            errors.push(ResolveError {
                message: format!(
                    "`{target}` is private to `{owning}`; mark it `pub` to use it outside"
                ),
                span: d.span,
            });
        }
    }
}

/// The module a qualified name lives in: everything before its last segment.
fn module_of(qualified: &str) -> String {
    match qualified.rsplit_once("::") {
        Some((path, _)) => path.to_string(),
        None => String::new(),
    }
}

/// Is a declaration of `owner` reachable from module `here` — the same
/// module, or one nested inside it?
fn visible_from(owner: &str, here: &str) -> bool {
    here == owner || here.starts_with(&format!("{owner}::"))
}

/// The qualified name a declaration introduces, if it introduces one.
fn declared_name(d: &Decl) -> Option<&String> {
    match d {
        Decl::Fn { name, .. }
        | Decl::Command { name, .. }
        | Decl::Data { name, .. }
        | Decl::Enum { name, .. }
        | Decl::Menu { name, .. }
        | Decl::Form { name, .. }
        | Decl::Const { name, .. }
        | Decl::Trait { name, .. }
        | Decl::Effect { name, .. } => Some(name),
        _ => None,
    }
}

/// Every qualified name a declaration mentions. A name with no `::` reaches
/// nothing a module hid, so only paths are collected.
fn references(d: &Decl, out: &mut Vec<String>) {
    fn ty(t: &TypeExpr, out: &mut Vec<String>) {
        match t {
            TypeExpr::Base(name) => push(name, out),
            TypeExpr::Apply(name, args) => {
                push(name, out);
                for a in args {
                    ty(&a.kind, out);
                }
            }
            TypeExpr::Positive(i) | TypeExpr::Negative(i) | TypeExpr::Dual(i) => ty(&i.kind, out),
            TypeExpr::Effectful(i, _) => ty(&i.kind, out),
            TypeExpr::Row(_) => {}
            TypeExpr::Tensor(items)
            | TypeExpr::Par(items)
            | TypeExpr::With(items)
            | TypeExpr::Sum(items) => {
                for item in items {
                    ty(&item.kind, out);
                }
            }
            TypeExpr::Fun(a, b) => {
                ty(&a.kind, out);
                ty(&b.kind, out);
            }
        }
    }
    fn push(name: &str, out: &mut Vec<String>) {
        if name.contains("::") {
            out.push(name.to_string());
        }
    }
    fn pattern(p: &Pattern, out: &mut Vec<String>) {
        match p {
            Pattern::Ident(name) => push(name, out),
            Pattern::Enum { name, .. } => push(name, out),
            Pattern::Data { name, fields } => {
                push(name, out);
                for (_, f) in fields {
                    pattern(f, out);
                }
            }
            Pattern::Binding { pattern: p, .. } => pattern(p, out),
            Pattern::Or(items) | Pattern::Tuple(items) | Pattern::Bundle(items) => {
                for i in items {
                    pattern(i, out);
                }
            }
            Pattern::Inject { pattern: p, .. } => pattern(p, out),
            Pattern::Dtor { arg, .. } => pattern(arg, out),
            _ => {}
        }
    }
    fn expr(e: &Node<Expr>, out: &mut Vec<String>) {
        match &e.kind {
            Expr::Ident(name) => push(name, out),
            Expr::Data { name, .. } => push(name, out),
            Expr::Select { ty: Some(t), arms } => {
                ty(&t.kind, out);
                for arm in arms {
                    pattern(&arm.pattern, out);
                }
            }
            Expr::Select { ty: None, arms } => {
                for arm in arms {
                    pattern(&arm.pattern, out);
                }
            }
            Expr::Match { arms, .. } => {
                for arm in arms {
                    pattern(&arm.pattern, out);
                }
            }
            Expr::Let { ty: Some(t), .. } => ty(t, out),
            _ => {}
        }
        for child in e.kind.children() {
            expr(child, out);
        }
    }
    match d {
        Decl::Fn { params, return_type, body, .. } => {
            for p in params {
                if let Some(t) = &p.ty {
                    ty(t, out);
                }
            }
            if let Some(t) = return_type {
                ty(t, out);
            }
            expr(body, out);
        }
        Decl::Command { value_params, continuation_params, body, .. } => {
            for p in value_params.iter().chain(continuation_params) {
                if let Some(t) = &p.ty {
                    ty(t, out);
                }
            }
            expr(body, out);
        }
        Decl::Const { ty: t, value, .. } => {
            ty(t, out);
            expr(value, out);
        }
        Decl::Data { fields, .. } | Decl::Form { fields, .. } => {
            for (_, t) in fields {
                ty(t, out);
            }
        }
        Decl::Menu { items, .. } => {
            for (_, t) in items {
                ty(t, out);
            }
        }
        Decl::Enum { variants, .. } => {
            for (_, payload) in variants {
                for t in payload {
                    ty(t, out);
                }
            }
        }
        _ => {}
    }
}

/// Apply `use Enum::*;` and `use Enum::{A, B};`: every bare use of an
/// imported variant — in patterns and in expressions — is rewritten to its
/// qualified label, so the automatic unqualified-while-unambiguous rule
/// never has to guess about it. An import that collides with another, or
/// names a variant its enum does not have, is an error at the `use`.
fn apply_variant_imports(
    decls: Vec<Node<Decl>>,
    units: &[usize],
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
    let mut tables: Vec<HashMap<String, String>> = vec![HashMap::new(); units.len() + 1];
    let unit = |span_start: usize| unit_of(span_start, units);
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
                message: format!("`use {enum_name}::…` names neither a module nor a declared enum"),
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
    let mut scope = Scope {
        type_parameters: Default::default(),
        path,
        declares: HashSet::new(),
        aliases: HashMap::new(),
        globs: HashMap::new(),
        ambiguous: std::cell::RefCell::new(Vec::new()),
    };
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
            Decl::Effect { name, operations, .. } => {
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
    modules: &Modules,
    errors: &mut Vec<ResolveError>,
) {
    for d in decls {
        match &d.kind {
            Decl::Mod { name, decls, .. } => {
                let mut path = stack.last().expect("a scope").path.clone();
                path.push(name.clone());
                let mut scope = collect_scope(decls, path, errors);
                expand_globs(&mut scope, decls, modules);
                stack.push(scope);
                flatten(decls, stack, out, modules, errors);
                stack.pop();
            }
            Decl::Use { path, imports } => {
                // A variant import survives flattening — with the enum it
                // names resolved to its flat name — and a later pass applies
                // it to the whole program. A glob over a module is not one:
                // its names are already in the scope.
                let over_module = matches!(imports, crate::ast::UseImports::Glob)
                    && find_module(path, &stack.last().expect("a scope").path, modules).is_some();
                if !matches!(imports, crate::ast::UseImports::Member) && !over_module {
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
                let mut reported = HashSet::new();
                for (name, candidates) in stack[0].ambiguous.borrow_mut().drain(..) {
                    if reported.insert(name.clone()) {
                        let listed: Vec<String> =
                            candidates.iter().map(|c| format!("`{c}`")).collect();
                        errors.push(ResolveError {
                            message: format!(
                                "`{name}` is brought in by more than one glob — {} — so \
                                 write the one you mean, or `use` it by name",
                                listed.join(", ")
                            ),
                            span: d.span,
                        });
                    }
                }
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
        if let Some(candidates) = scope.globs.get(first) {
            if let [target] = candidates.as_slice() {
                return match rest {
                    Some(rest) => format!("{target}::{rest}"),
                    None => target.clone(),
                };
            }
            stack[0].ambiguous.borrow_mut().push((first.to_string(), candidates.clone()));
            return written.to_string();
        }
    }
    written.to_string()
}

fn resolve_decl(d: &mut Decl, stack: &[Scope], locals: &mut Vec<HashSet<String>>) {
    let scope = stack.last().expect("a scope");
    let parameters = match &*d {
        Decl::Fn { type_params, .. }
        | Decl::Command { type_params, .. }
        | Decl::Data { type_params, .. }
        | Decl::Enum { type_params, .. }
        | Decl::Menu { type_params, .. }
        | Decl::Form { type_params, .. }
        | Decl::Effect { type_params, .. }
        | Decl::Trait { type_params, .. }
        | Decl::Impl { type_params, .. } => type_params.iter().cloned().collect(),
        _ => HashSet::new(),
    };
    scope.type_parameters.borrow_mut().push(parameters);
    match d {
        Decl::Fn { name, params, return_type, body, effects, bounds, .. } => {
            *name = scope.qualify(name);
            resolve_row(effects, stack);
            resolve_bounds(bounds, stack);
            let mut bound = HashSet::new();
            for p in params.iter_mut() {
                resolve_param(p, stack);
                bound.extend(pattern_binders(&p.pattern));
            }
            if let Some(ty) = return_type {
                resolve_type(ty, stack);
            }
            locals.push(bound);
            resolve_expr(&mut body.kind, stack, locals);
            locals.pop();
        }
        Decl::Command {
            name,
            value_params,
            continuation_params,
            return_type,
            body,
            effects,
            bounds,
            ..
        } => {
            *name = scope.qualify(name);
            resolve_row(effects, stack);
            resolve_bounds(bounds, stack);
            let mut bound = HashSet::new();
            for p in value_params.iter_mut().chain(continuation_params.iter_mut()) {
                resolve_param(p, stack);
                bound.extend(pattern_binders(&p.pattern));
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
        Decl::Menu { name, items, effects, .. } => {
            *name = scope.qualify(name);
            resolve_row(effects, stack);
            for (_, ty) in items {
                resolve_type(ty, stack);
            }
        }
        Decl::Form { name, fields, effects, .. } => {
            *name = scope.qualify(name);
            resolve_row(effects, stack);
            for (_, ty) in fields {
                resolve_type(ty, stack);
            }
        }
        Decl::Const { name, ty, value, .. } => {
            *name = scope.qualify(name);
            resolve_type(ty, stack);
            resolve_expr(&mut value.kind, stack, locals);
        }
        Decl::Trait { name, methods, supers, assocs, .. } => {
            *name = scope.qualify(name);
            for parent in supers.iter_mut() {
                parent.trait_name = resolve_name(&parent.trait_name, stack);
                for arg in &mut parent.args {
                    resolve_type(arg, stack);
                }
            }
            // `Self` and each associated name are types in signatures and
            // default bodies. They stay bare: qualifying `Item` would make
            // it a different name from the one the impl substitutes.
            if let Some(params) = scope.type_parameters.borrow_mut().last_mut() {
                params.insert("Self".to_string());
                for item in assocs.iter() {
                    params.insert(item.clone());
                }
            }
            for m in methods {
                for p in m.value_params.iter_mut().chain(m.continuation_params.iter_mut()) {
                    resolve_param(p, stack);
                }
                if let Some(ty) = &mut m.return_type {
                    resolve_type(ty, stack);
                }
                if let Some(body) = &mut m.body {
                    let mut bound = HashSet::new();
                    for p in m.value_params.iter().chain(m.continuation_params.iter()) {
                        bound.extend(pattern_binders(&p.pattern));
                    }
                    locals.push(bound);
                    resolve_expr(&mut body.kind, stack, locals);
                    locals.pop();
                }
            }
        }
        Decl::Impl { trait_name, trait_args, for_type, bounds, assocs, methods, .. } => {
            *trait_name = resolve_name(trait_name, stack);
            for arg in trait_args {
                resolve_type(arg, stack);
            }
            resolve_bounds(bounds, stack);
            resolve_type(for_type, stack);
            // `Self` in `type Item = Self` is the implementing type. A trait
            // parameter written by name stays bare when nothing declares it,
            // and elaboration substitutes it for the argument the impl gave.
            if let Some(params) = scope.type_parameters.borrow_mut().last_mut() {
                params.insert("Self".to_string());
            }
            for (_, ty) in assocs.iter_mut() {
                resolve_type(ty, stack);
            }
            for method in methods {
                // A method is named by its trait, not by the module the impl
                // sits in: `fmt` stays `fmt` inside `mod list`, or the
                // elaboration would not know it implements `Display::fmt`.
                let unqualified = match &method.kind {
                    Decl::Fn { name, .. } | Decl::Command { name, .. } => name.clone(),
                    _ => String::new(),
                };
                resolve_decl(&mut method.kind, stack, locals);
                match &mut method.kind {
                    Decl::Fn { name, .. } | Decl::Command { name, .. } => *name = unqualified,
                    _ => {}
                }
            }
        }
        Decl::Effect { name, operations, .. } => {
            *name = scope.qualify(name);
            // An operation is named by the module it is declared in, as a
            // function is: references to it, and a handler's clauses for it,
            // resolve to the same qualified name.
            for op in operations {
                op.name = scope.qualify(&op.name);
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
    scope.type_parameters.borrow_mut().pop();
}

fn resolve_bounds(bounds: &mut [crate::ast::TraitBound], stack: &[Scope]) {
    for bound in bounds {
        bound.trait_name = resolve_name(&bound.trait_name, stack);
        for arg in &mut bound.args {
            resolve_type(arg, stack);
        }
        for pin in &mut bound.pins {
            resolve_type(&mut pin.ty, stack);
        }
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
            *name = resolve_type_name(name, stack);
            for arg in args {
                resolve_type(&mut arg.kind, stack);
            }
        }
        TypeExpr::Base(name) => *name = resolve_type_name(name, stack),
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) | TypeExpr::Dual(inner) => {
            resolve_type(&mut inner.kind, stack)
        }
        TypeExpr::Tensor(items)
        | TypeExpr::Par(items)
        | TypeExpr::With(items)
        | TypeExpr::Sum(items) => {
            for item in items {
                resolve_type(&mut item.kind, stack);
            }
        }
        TypeExpr::Fun(a, b) => {
            resolve_type(&mut a.kind, stack);
            resolve_type(&mut b.kind, stack);
        }
        TypeExpr::Effectful(inner, row) => {
            resolve_type(&mut inner.kind, stack);
            resolve_row(row, stack);
        }
        TypeExpr::Row(row) => resolve_row(row, stack),
    }
}

fn resolve_type_name(name: &str, stack: &[Scope]) -> String {
    if stack
        .iter()
        .any(|scope| scope.type_parameters.borrow().iter().any(|params| params.contains(name)))
    {
        name.to_string()
    } else {
        resolve_name(name, stack)
    }
}

/// Qualify the effects a row names, as a type name is qualified. A row
/// variable is a generic parameter of the declaration, and stays as written.
fn resolve_row(row: &mut EffectRow, stack: &[Scope]) {
    for effect in row.effects.iter_mut() {
        match &mut effect.kind {
            TypeExpr::Base(name) => *name = resolve_name(name, stack),
            TypeExpr::Apply(name, args) => {
                *name = resolve_name(name, stack);
                for argument in args {
                    resolve_type(&mut argument.kind, stack);
                }
            }
            _ => resolve_type(&mut effect.kind, stack),
        }
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
                bound.extend(pattern_binders(&p.pattern));
            }
            locals.push(bound);
            resolve_expr(&mut body.kind, stack, locals);
            locals.pop();
        }
        Expr::Let { pattern, ty, value, body, .. } => {
            if let Some(ty) = ty {
                resolve_type(ty, stack);
            }
            resolve_expr(&mut value.kind, stack, locals);
            resolve_pattern(pattern, stack, locals);
            locals.push(pattern_binders(pattern));
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
                if let Expr::Let { pattern, body: None, .. } = &item.kind {
                    locals.push(pattern_binders(pattern));
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
        Expr::Inject { value, .. } => resolve_expr(&mut value.kind, stack, locals),
        Expr::Pair(items)
        | Expr::Bundle(items)
        | Expr::Par(items)
        | Expr::Flow { stages: items, .. } => {
            for item in items {
                resolve_expr(&mut item.kind, stack, locals);
            }
        }
        Expr::Project { base: body, .. } => {
            resolve_expr(&mut body.kind, stack, locals);
        }
        Expr::Handle { .. } | Expr::Handler { .. } => {
            let (clauses, ret) = match e {
                Expr::Handle { body, clauses, ret, .. } => {
                    resolve_expr(&mut body.kind, stack, locals);
                    (clauses, ret)
                }
                Expr::Handler { effects, clauses, ret, .. } => {
                    for effect in effects {
                        *effect = resolve_name(effect, stack);
                    }
                    (clauses, ret)
                }
                _ => unreachable!(),
            };
            for c in clauses.iter_mut() {
                c.op = resolve_name(&c.op, stack);
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
        Expr::WithHandler { handler, body } => {
            resolve_expr(&mut handler.kind, stack, locals);
            resolve_expr(&mut body.kind, stack, locals);
        }
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) => {}
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
        Pattern::Inject { pattern, .. } => resolve_pattern(pattern, stack, locals),
        Pattern::Range { start, end } => {
            resolve_pattern(start, stack, locals);
            resolve_pattern(end, stack, locals);
        }
        Pattern::Ident(_)
        | Pattern::Wildcard
        | Pattern::Int(_)
        | Pattern::Str(_)
        | Pattern::Char(_)
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
        Pattern::Dtor { arg, .. } | Pattern::Inject { pattern: arg, .. } => {
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
                rewrite_expr_imports(&mut arm.body.kind, imported);
            }
        }
        Expr::Select { arms, .. } | Expr::CoMatch { arms, .. } => {
            for arm in arms {
                rewrite_pattern_imports(&mut arm.pattern, imported);
                rewrite_expr_imports(&mut arm.command.kind, imported);
            }
        }
        Expr::Lambda { body, .. } | Expr::Mu { body, .. } => {
            rewrite_expr_imports(&mut body.kind, imported)
        }
        Expr::Call { callee, args } => {
            rewrite_expr_imports(&mut callee.kind, imported);
            for arg in args {
                rewrite_expr_imports(&mut arg.kind, imported);
            }
        }
        Expr::Inject { value, .. } => rewrite_expr_imports(&mut value.kind, imported),
        Expr::Pair(items)
        | Expr::Bundle(items)
        | Expr::Par(items)
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
        Expr::Request { arg: expr, .. } => rewrite_expr_imports(&mut expr.kind, imported),
        Expr::Project { base, .. } => rewrite_expr_imports(&mut base.kind, imported),
        Expr::Handle { .. } | Expr::Handler { .. } => {
            let (clauses, ret) = match e {
                Expr::Handle { body, clauses, ret, .. } => {
                    rewrite_expr_imports(&mut body.kind, imported);
                    (clauses, ret)
                }
                Expr::Handler { clauses, ret, .. } => (clauses, ret),
                _ => unreachable!(),
            };
            for clause in clauses {
                rewrite_expr_imports(&mut clause.body.kind, imported);
            }
            if let Some((_, ret)) = ret {
                rewrite_expr_imports(&mut ret.kind, imported);
            }
        }
        Expr::WithHandler { handler, body } => {
            rewrite_expr_imports(&mut handler.kind, imported);
            rewrite_expr_imports(&mut body.kind, imported);
        }
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) => {}
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
        Pattern::Binding { pattern, .. } | Pattern::Inject { pattern, .. } => {
            rewrite_pattern_imports(pattern, imported)
        }
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
