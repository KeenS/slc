//! Expression type checking for the ergonomic surface syntax.
//!
//! This pass is intentionally concrete: it validates boolean operators,
//! numeric/comparison operators, indexing, patterns, guards, and annotated
//! bindings using known literal and declaration types. It is not a
//! replacement for the core inference pass; it catches the surface-syntax
//! mistakes that used to appear only at runtime.

use crate::declarations::{Declarations, enum_types};
use crate::env::{Env, constant_types};
use crate::signatures::{FunctionSignature, function_types, instantiate, is_builtin};
use slc_core::types::{Base, Type};
use slc_core::typing::contains_var;
use slc_syntax::ast::{Decl, Expr, Named, Node, Program, TypeExpr};
use slc_syntax::lower::lower_type;
use slc_syntax::token::Span;
use slc_syntax::traits::TraitInfo;
use std::collections::HashMap;

pub use crate::Diagnostic;

pub fn check_program(p: &Program, traits: &TraitInfo) -> Result<(), Vec<Diagnostic>> {
    check_program_resolving(p, traits).map(|_| ())
}

/// Type-check the program and, on success, return how its trait dispatch
/// resolves: per method call, a direct impl or a dictionary projection; per
/// bounded-function call, the dictionaries to pass. Lowering uses this to
/// compile traits without any runtime method value.
pub fn check_program_resolving(
    p: &Program,
    traits: &TraitInfo,
) -> Result<slc_syntax::lower::DispatchInfo, Vec<Diagnostic>> {
    let constants = constant_types(p);
    let enums = enum_types(p);
    let functions = function_types(p, &enums);
    let mut diags = Vec::new();
    let mut env = Env::root(&constants, &functions, traits);
    for d in &p.decls {
        check_decl(d, &enums, &mut env, &mut diags);
    }
    if diags.is_empty() { Ok(std::mem::take(&mut env.dispatch)) } else { Err(diags) }
}

/// The type key a resolved type dispatches on — matching the runtime's key
/// and `slc_syntax::traits::type_key`.
fn type_key(ty: &Type) -> Option<String> {
    match ty {
        Type::Pos(b) | Type::Neg(b) => Some(format!("{b}")),
        Type::Named(n) => Some(n.clone()),
        Type::List(_) => Some("list".into()),
        Type::Down(t) | Type::Up(t) | Type::Dual(t) => type_key(t),
        _ => None,
    }
}

/// Check that `target` satisfies `trait_name`: a ground type must have an
/// impl; a bound rigid variable is covered by the enclosing declaration; an
/// unsolved variable at a monomorphic call cannot be discharged.
fn discharge_bound(
    trait_name: &str,
    target: &Type,
    callee: &str,
    span: Span,
    env: &Env,
    diags: &mut Vec<Diagnostic>,
) {
    if let Type::Var(v) = target {
        if env.bounds.iter().any(|(bv, bt, _)| bv == v && bt == trait_name) {
            return;
        }
        diags.push(Diagnostic {
            message: format!(
                "`{callee}` needs `{trait_name}` for a type parameter, but the caller's type is                  not known to satisfy it"
            ),
            span,
        });
        return;
    }
    match type_key(target) {
        Some(key) if env.traits.has_impl(trait_name, &key) => {}
        Some(key) => {
            diags.push(Diagnostic { message: format!("no `impl {trait_name} for {key}`"), span })
        }
        None => diags.push(Diagnostic {
            message: format!("`{trait_name}` cannot be required of {target}"),
            span,
        }),
    }
}

/// Type a trait-method call. `Self` becomes a fresh variable unified with the
/// first argument; the method's declared parameter and result types, with
/// `Self` substituted, type the rest; the bound is then discharged.
fn check_trait_method_call(
    method: &str,
    args: &[Node<Expr>],
    span: Span,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    let sig = env.traits.method_sig(method)?.clone();
    let trait_name = env.traits.method_owner.get(method)?.clone();
    let self_ty = env.uni.fresh_var();
    let params: Vec<&slc_syntax::ast::Param> =
        sig.value_params.iter().chain(sig.continuation_params.iter()).collect();
    for (arg, param) in args.iter().zip(params.iter()) {
        let Some(expected) =
            param.ty.as_ref().and_then(|ty| resolve_with_self(ty, &self_ty, enums))
        else {
            check_expr(arg, enums, env, diags);
            continue;
        };
        if let Some(actual) = check_expr(arg, enums, env, diags)
            && !fits(env, &expected, &actual, &arg.kind)
        {
            let expected = env.uni.apply(&expected);
            diags.push(Diagnostic {
                message: format!("argument to `{method}` has type {actual}; expected {expected}"),
                span: arg.span,
            });
        }
    }
    // Discharge `Self: Trait` against what the first argument fixed it to.
    let target = env.uni.apply(&self_ty);
    discharge_bound(&trait_name, &target, method, span, env, diags);
    // Resolve the dispatch for lowering. A concrete receiver calls the impl
    // directly; a bounded type parameter projects the method from the
    // enclosing function's dictionary for that bound.
    let resolution = match &target {
        Type::Var(v) => env.bounds.iter().find(|(bv, bt, _)| bv == v && bt == &trait_name).map(
            |(_, _, type_param)| {
                let methods = env.traits.traits.get(&trait_name);
                let count = methods.map(|m| m.len()).unwrap_or(1);
                let index =
                    methods.and_then(|m| m.iter().position(|tm| tm.name == method)).unwrap_or(0);
                slc_syntax::lower::MethodDispatch::Dict {
                    dict_var: slc_syntax::lower::dict_param_name(&trait_name, type_param),
                    index,
                    count,
                }
            },
        ),
        _ => type_key(&target)
            .and_then(|key| env.traits.method_impls.get(method).and_then(|m| m.get(&key)))
            .map(|mangled| slc_syntax::lower::MethodDispatch::Static(mangled.clone())),
    };
    if let Some(resolution) = resolution {
        env.dispatch.methods.insert(span, resolution);
    }
    // A command method returns bottom; a fn method returns its (Self-subst)
    // result type.
    if sig.is_command {
        return Some(Type::Bottom);
    }
    match &sig.return_type {
        Some(ty) => resolve_with_self(ty, &self_ty, enums).map(|t| env.uni.apply(&t)),
        None => Some(Type::One),
    }
}

/// Resolve a method's written type, mapping `Self` to the call's Self
/// variable and everything else through the ordinary declaration resolver.
fn resolve_with_self(ty: &TypeExpr, self_ty: &Type, enums: &Declarations) -> Option<Type> {
    use slc_syntax::ast::TypeExpr as T;
    match ty {
        T::Base(name) if name == "Self" => Some(self_ty.clone()),
        T::Positive(inner) => resolve_with_self(&inner.kind, self_ty, enums),
        T::Negative(inner) if !matches!(inner.kind, T::Bottom) => {
            Some(resolve_with_self(&inner.kind, self_ty, enums)?.dual())
        }
        T::Dual(inner) => Some(resolve_with_self(&inner.kind, self_ty, enums)?.dual()),
        T::Down(inner) => {
            Some(Type::Down(Box::new(resolve_with_self(&inner.kind, self_ty, enums)?)))
        }
        T::Up(inner) => Some(Type::Up(Box::new(resolve_with_self(&inner.kind, self_ty, enums)?))),
        _ => enums.resolve(ty),
    }
}

/// The type a `select`'s arms name, when one of them does: a struct pattern
/// names its struct, `Color::Red(x)` its enum, and a bare `Red` the enum that
/// declares it.
fn named_by_arms(arms: &[slc_syntax::ast::SelectArm], enums: &Declarations) -> Option<Type> {
    arms.iter().find_map(|arm| match arm.pattern.names()? {
        Named::Declaration(name) if enums.declares(name) => Some(Type::Named(name.to_string())),
        Named::Variant(name) => {
            enums.variant(name).map(|(declaration, _)| Type::Named(declaration.clone()))
        }
        Named::Declaration(_) => None,
    })
}

/// The type an unannotated local-`mu` parameter has, read off the body that
/// uses it. Two shapes say it outright: a call that hands the parameter to a
/// slot whose type the callee declares, and a cut that sends a value to it.
fn infer_param_type(
    name: &str,
    body: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
) -> Option<Type> {
    if let Expr::Call { callee, args } = &body.kind
        && let Expr::Ident(function) = &callee.kind
        && let Some(slot) =
            args.iter().position(|a| matches!(&a.kind, Expr::Ident(x) if x == name))
        && let Some(ty) = env.functions.get(function).and_then(|s| s.params.get(slot)).cloned()
        // A template variable in a signature is per-call; it says nothing
        // about this parameter.
        && !contains_var(&ty)
    {
        return Some(ty);
    }
    if let Expr::Cut { value, consumer } = &body.kind
        && matches!(&consumer.kind, Expr::Ident(x) if x == name)
    {
        // `v @ k` makes `k` the consumer of whatever `v` is. The value is
        // checked again in place, so these diagnostics are thrown away.
        return check_expr(value, enums, env, &mut Vec::new()).map(|ty| ty.dual());
    }
    body.kind.children().into_iter().find_map(|child| infer_param_type(name, child, enums, env))
}

/// Resolve a declared type, substituting a rigid variable for each type
/// parameter even under a sign or a shift: `+T` and `↓-T` find `T` too.
fn resolve_rigid(
    ty: &TypeExpr,
    rigid_vars: &HashMap<&str, Type>,
    enums: &Declarations,
) -> Option<Type> {
    use slc_syntax::ast::TypeExpr as T;
    match ty {
        T::Base(name) => rigid_vars.get(name.as_str()).cloned().or_else(|| enums.resolve(ty)),
        T::Positive(inner) => resolve_rigid(&inner.kind, rigid_vars, enums),
        T::Negative(inner) if !matches!(inner.kind, T::Bottom) => {
            Some(resolve_rigid(&inner.kind, rigid_vars, enums)?.dual())
        }
        T::Dual(inner) => Some(resolve_rigid(&inner.kind, rigid_vars, enums)?.dual()),
        T::Down(inner) => {
            Some(Type::Down(Box::new(resolve_rigid(&inner.kind, rigid_vars, enums)?)))
        }
        T::Up(inner) => Some(Type::Up(Box::new(resolve_rigid(&inner.kind, rigid_vars, enums)?))),
        other => enums.resolve(other),
    }
}

/// Put a declaration's bounds in scope for its body, as (rigid-variable
/// index, trait, type-parameter name), and return the previous set to
/// restore afterward.
fn record_bounds(
    bounds: &[(String, String)],
    rigid_vars: &HashMap<&str, Type>,
    env: &mut Env,
) -> Vec<(usize, String, String)> {
    let outer = env.bounds.clone();
    for (var, trait_name) in bounds {
        if let Some(Type::Var(v)) = rigid_vars.get(var.as_str()) {
            env.bounds.push((*v, trait_name.clone(), var.clone()));
        }
    }
    outer
}

fn check_decl(d: &Node<Decl>, enums: &Declarations, env: &mut Env, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { name, params, body, polarity, return_type, type_params, bounds, .. } => {
            env.push();
            // A type parameter is rigid inside the body: `T` is some type the
            // caller chose, not a licence to treat the value as any type.
            let rigid_vars: HashMap<&str, Type> =
                type_params.iter().map(|tp| (tp.as_str(), env.uni.fresh_rigid())).collect();
            let outer_bounds = record_bounds(bounds, &rigid_vars, env);
            let rigid = |ty: &TypeExpr| resolve_rigid(ty, &rigid_vars, enums);
            for p in params {
                if let Some(ty) = p.ty.as_ref().and_then(&rigid) {
                    env.define(&p.name, ty);
                }
            }
            // A negative function produces the consumer of what follows its
            // `<-`, so that is what a `select` in its body consumes.
            let outer = env.consumed.take();
            let declared = return_type.as_ref().and_then(&rigid);
            env.consumed = (*polarity == slc_syntax::ast::FunctionPolarity::Negative)
                .then(|| declared.clone())
                .flatten();
            let body_type = check_expr(body, enums, env, diags);
            env.consumed = outer;
            env.bounds = outer_bounds;
            env.pop();
            // The body produces what the declaration promises: the return
            // type for `->`, its consumer for `<-`. A body that ends in a
            // cut produces nothing and promises nothing.
            let promised = match polarity {
                slc_syntax::ast::FunctionPolarity::Positive => declared,
                slc_syntax::ast::FunctionPolarity::Negative => declared.map(|ty| ty.dual()),
            };
            if let (Some(promised), Some(actual)) = (&promised, &body_type)
                && actual != &Type::Bottom
                && !fits(env, promised, actual, tail_expr(&body.kind))
            {
                diags.push(Diagnostic {
                    message: format!(
                        "the body of `{name}` has type {actual}; the declaration says {promised}"
                    ),
                    span: body.span,
                });
            }
        }
        Decl::Command { value_params, continuation_params, body, type_params, bounds, .. } => {
            env.push();
            let rigid_vars: HashMap<&str, Type> =
                type_params.iter().map(|tp| (tp.as_str(), env.uni.fresh_rigid())).collect();
            let outer_bounds = record_bounds(bounds, &rigid_vars, env);
            for p in value_params.iter().chain(continuation_params.iter()) {
                if let Some(ty) = p.ty.as_ref().and_then(|ty| resolve_rigid(ty, &rigid_vars, enums))
                {
                    env.define(&p.name, ty);
                }
            }
            check_expr(body, enums, env, diags);
            env.bounds = outer_bounds;
            env.pop();
        }
        Decl::Const { name, ty, value } => {
            if !is_constant_initializer(&value.kind, env) {
                diags.push(Diagnostic {
                    message: format!(
                        "const `{name}` initializer must be a literal or another constant"
                    ),
                    span: value.span,
                });
            }
            let expected = lower_type(ty);
            let actual = infer_expr(value, enums, env, diags);
            if let (Ok(expected), Some(actual)) = (&expected, actual)
                && &actual != expected
            {
                diags.push(Diagnostic {
                    message: format!(
                        "const `{name}` is annotated as {expected}; initializer has type {actual}"
                    ),
                    span: value.span,
                });
            }
        }
        _ => {}
    }
}

/// The value restriction: a `let` of a syntactic value ran nothing, so no
/// two instantiations can disagree about anything that happened —
/// generalize its own variables. Anything that computes — `mu` above all —
/// stays monomorphic.
fn generalize(
    binding_ty: Type,
    value: &Expr,
    enums: &Declarations,
    env: &mut Env,
) -> (Type, Vec<usize>) {
    let binding_ty = env.uni.apply(&binding_ty);
    if !is_value_form(value, enums) {
        return (binding_ty, Vec::new());
    }
    let claimed = env.free_vars();
    let mut own = std::collections::HashSet::new();
    crate::env::collect_vars(&binding_ty, &mut own);
    let generalized =
        own.into_iter().filter(|v| !claimed.contains(v) && !env.uni.is_rigid(*v)).collect();
    (binding_ty, generalized)
}

/// The value restriction's syntactic class: an expression that evaluates
/// without running anything. `mu` is the definitive non-member — it captures
/// — and so is every application, which may run a command or hand back a
/// value holding a captured continuation.
fn is_value_form(e: &Expr, enums: &Declarations) -> bool {
    match e {
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Char(_)
        | Expr::Bool(_)
        | Expr::Ident(_)
        | Expr::Lambda { .. }
        | Expr::Select { .. } => true,
        Expr::Pair(items) => items.iter().all(|item| is_value_form(&item.kind, enums)),
        Expr::Struct { fields, .. } => {
            fields.iter().all(|(_, value)| is_value_form(&value.kind, enums))
        }
        // A constructor applied to values builds data; any other call runs.
        Expr::Call { callee, args } => {
            matches!(&callee.kind, Expr::Ident(name) if enums.variant(name).is_some())
                && args.iter().all(|arg| is_value_form(&arg.kind, enums))
        }
        // The shifts are erased coercions: a boxed value is a value.
        Expr::Shift { expr, .. } => is_value_form(&expr.kind, enums),
        _ => false,
    }
}

fn is_constant_initializer(e: &Expr, env: &Env) -> bool {
    match e {
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) | Expr::Bool(_) => true,
        Expr::Ident(name) => env.constants.contains_key(name),
        _ => false,
    }
}

fn literal_type(e: &Expr) -> Option<Type> {
    Some(match e {
        Expr::Int(_) => Type::Pos(Base::I64),
        Expr::Float(_) => Type::One,
        Expr::Str(_) => Type::Pos(Base::Str),
        Expr::Char(_) => Type::Pos(Base::Char),
        Expr::Bool(_) => Type::Pos(Base::Bool),
        _ => return None,
    })
}

fn pattern_type(pattern: &slc_syntax::ast::Pattern) -> Option<Type> {
    use slc_syntax::ast::Pattern;
    Some(match pattern {
        Pattern::Int(_) => Type::Pos(Base::I64),
        Pattern::Str(_) => Type::Pos(Base::Str),
        Pattern::Char(_) => Type::Pos(Base::Char),
        Pattern::Bool(_) => Type::Pos(Base::Bool),
        Pattern::Float(_) => Type::One,
        Pattern::Range { start, .. } => pattern_type(start)?,
        Pattern::Or(alternatives) => {
            let first = pattern_type(alternatives.first()?)?;
            if alternatives.iter().all(|p| pattern_type(p) == Some(first.clone())) {
                first
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

fn check_pattern(
    pattern: &slc_syntax::ast::Pattern,
    expected: &Type,
    declarations: &Declarations,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use slc_syntax::ast::Pattern;
    let actual = pattern_type(pattern);
    if let Some(actual) = actual
        && &actual != expected
    {
        diags.push(Diagnostic {
            message: format!("pattern has type {actual}; scrutinee has type {expected}"),
            span,
        });
    }
    match pattern {
        Pattern::Or(alternatives) => {
            for alternative in alternatives {
                check_pattern(alternative, expected, declarations, span, diags);
            }
        }
        Pattern::Range { start, end } => {
            check_pattern(start, expected, declarations, span, diags);
            check_pattern(end, expected, declarations, span, diags);
        }
        Pattern::Binding { pattern, .. } => {
            check_pattern(pattern, expected, declarations, span, diags)
        }
        Pattern::Tuple(items) => {
            for item in items {
                check_pattern(item, expected, declarations, span, diags);
            }
        }
        // A struct pattern decomposes the product: the same fields, in the
        // same order, with the same types as the declaration.
        Pattern::Struct { name, fields } => {
            let Some(declared) = declarations.structs.get(name) else {
                diags.push(Diagnostic {
                    message: format!("`{name}` is not a declared struct"),
                    span,
                });
                return;
            };
            if expected != &Type::Named(name.clone()) {
                diags.push(Diagnostic {
                    message: format!(
                        "struct pattern `{name}` cannot match a scrutinee of type {expected}"
                    ),
                    span,
                });
            }
            let written: Vec<&String> = fields.iter().map(|(field, _)| field).collect();
            let expected_fields: Vec<&String> = declared.iter().map(|(field, _)| field).collect();
            if written != expected_fields {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` has fields {}; the pattern writes {}",
                        expected_fields
                            .iter()
                            .map(|f| format!("`{f}`"))
                            .collect::<Vec<_>>()
                            .join(", "),
                        if written.is_empty() {
                            "none".to_string()
                        } else {
                            written.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
                        }
                    ),
                    span,
                });
            }
            for (field, pattern) in fields {
                if let Some((_, field_ty)) = declared.iter().find(|(declared, _)| declared == field)
                {
                    check_pattern(pattern, field_ty, declarations, span, diags);
                }
            }
        }
        Pattern::List { items, rest } => {
            let expected = match expected {
                Type::List(item) => (**item).clone(),
                _ => {
                    diags.push(Diagnostic {
                        message: format!("list pattern expects a list, found {expected}"),
                        span,
                    });
                    Type::One
                }
            };
            for item in items {
                check_pattern(item, &expected, declarations, span, diags);
            }
            if let Some(rest) = rest {
                check_pattern(
                    rest,
                    &Type::List(Box::new(expected.clone())),
                    declarations,
                    span,
                    diags,
                );
            }
        }
        _ => {}
    }
}

fn expected_index_type(value_ty: &Type) -> Option<Type> {
    match value_ty {
        Type::Pos(Base::Str) | Type::List(_) => Some(Type::Pos(Base::I64)),
        _ => None,
    }
}

fn index_result_type(value_ty: &Type) -> Option<Type> {
    match value_ty {
        Type::Pos(Base::Str) => Some(Type::Pos(Base::Char)),
        Type::List(inner) => Some((**inner).clone()),
        _ => None,
    }
}

/// Does a value written as `expr`, inferred as `actual`, fit a port that
/// requires `expected`?
///
/// An integer literal takes the integer type its port requires — `0 @ exit`
/// sends an `i32` — and is `+i64` only when nothing constrains it. Every
/// other value must match its port exactly.
/// The expression a body's value comes from: the tail of a block, through a
/// trailing `let` — where an integer literal earns its adaptation.
fn tail_expr(e: &Expr) -> &Expr {
    match e {
        Expr::Block(items) => items.last().map(|n| tail_expr(&n.kind)).unwrap_or(e),
        Expr::Let { body: Some(body), .. } => tail_expr(&body.kind),
        _ => e,
    }
}

fn fits(env: &mut Env, expected: &Type, actual: &Type, expr: &Expr) -> bool {
    // A value that never arrives constrains nothing.
    if actual == &Type::Bottom {
        return true;
    }
    if env.uni.unify(expected, actual).is_ok() {
        return true;
    }
    // An integer literal takes the width its port requires.
    is_integer_literal(expr) && is_numeric(&env.uni.apply(expected)) && is_numeric(actual)
}

fn is_integer_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) => true,
        Expr::UnOp { op: slc_syntax::ast::UnOp::Neg, body } => is_integer_literal(&body.kind),
        _ => false,
    }
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Pos(Base::I32 | Base::I64 | Base::U32 | Base::U64))
}

fn is_comparable(ty: &Type) -> bool {
    is_numeric(ty)
        || matches!(ty, Type::Pos(Base::Char) | Type::Pos(Base::Str) | Type::Pos(Base::Bool))
}

/// A declaration's continuation row is positional and invariant: the
/// continuation supplied for a row position must have exactly the declared
/// type, and no position may be added, dropped, or reordered. Each
/// continuation is consumed exactly once, so a row that differs in width or
/// order is a different linear behavior, not a compatible one. Value
/// arguments are checked against their declared types the same way —
/// builtins excepted, whose arguments the builtin table already checks.
fn check_call_arguments(
    name: &str,
    signature: &FunctionSignature,
    args: &[Node<Expr>],
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let row_width = signature.continuations.iter().filter(|is_cont| **is_cont).count();
    if row_width > 0 && args.len() > signature.params.len() {
        diags.push(Diagnostic {
            message: format!(
                "`{name}` declares {} parameters including a continuation row of {row_width}; \
                 the call supplies {} arguments",
                signature.params.len(),
                args.len()
            ),
            span: args[signature.params.len()].span,
        });
        return;
    }
    for (index, arg) in args.iter().enumerate() {
        // Every argument is checked, whether or not the signature has a slot
        // for it — `println` takes anything, and what it takes may itself be
        // a call.
        let actual = check_expr(arg, enums, env, diags);
        let in_row = signature.continuations.get(index) == Some(&true);
        if !in_row && is_builtin(name) {
            continue;
        }
        let Some(expected) = signature.params.get(index) else {
            continue;
        };
        let Some(actual) = actual else {
            continue;
        };
        // `Type::One` is this checker's "not determined" placeholder — an
        // unannotated `let` binding, for instance. A mismatch is only
        // reported for an argument whose type is actually known.
        if !fits(env, expected, &actual, &arg.kind) {
            let expected = &env.uni.apply(expected);
            let message = if in_row {
                format!(
                    "continuation row mismatch: argument {} of `{name}` has type {actual}; \
                     the row declares {expected} at that position",
                    index + 1
                )
            } else {
                format!(
                    "argument {} of `{name}` has type {actual}; the declaration says {expected}",
                    index + 1
                )
            };
            diags.push(Diagnostic { message, span: arg.span });
        }
    }
}

/// Check a `let` initializer against its optional annotation and return the
/// type to bind. Both surface `let` forms — the expression form and the
/// bodyless form that scopes over the rest of a block — check identically.
fn check_let_binding(
    name: &str,
    ty: &Option<TypeExpr>,
    value: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Type {
    let actual = check_expr(value, enums, env, diags);
    let annotation = ty.as_ref().and_then(|ty| enums.resolve(ty));
    if let (Some(annotation), Some(actual)) = (&annotation, actual.clone())
        && !fits(env, annotation, &actual, &value.kind)
    {
        diags.push(Diagnostic {
            message: format!(
                "`let {name}` is annotated as {annotation}; initializer has type {actual}"
            ),
            span: value.span,
        });
    }
    // A binder the checker cannot type is a variable its uses will solve,
    // never a wildcard.
    annotation.or(actual).unwrap_or_else(|| env.uni.fresh_var())
}

/// Bind a `match` pattern's variables with their declared types, silently
/// and best-effort: exhaustiveness and shape are checked elsewhere, so this
/// only needs to get the types right where it can.
fn bind_match_pattern(
    pattern: &slc_syntax::ast::Pattern,
    scrutinee: &Type,
    enums: &Declarations,
    env: &mut Env,
) {
    use slc_syntax::ast::Pattern;
    match pattern {
        Pattern::Ident(name) => {
            // A bare name that is not a payload-less variant binds the value.
            if enums.variant(name).is_none() {
                env.define(name, scrutinee.clone());
            }
        }
        Pattern::Binding { name, pattern } => {
            env.define(name, scrutinee.clone());
            bind_match_pattern(pattern, scrutinee, enums, env);
        }
        Pattern::Enum { name, variant, fields } => {
            let written =
                if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
            let payload = enums.variant(&written).map(|(_, p)| p.clone()).unwrap_or_default();
            for (field, ty) in fields.iter().zip(payload.iter()) {
                bind_match_pattern(field, ty, enums, env);
            }
        }
        Pattern::Struct { name, fields } => {
            let declared = enums.fields(name).unwrap_or_default();
            for ((_, field), ty) in fields.iter().zip(declared.iter()) {
                bind_match_pattern(field, ty, enums, env);
            }
        }
        Pattern::Tuple(items) => {
            let components = flatten_tensor(scrutinee);
            for (item, ty) in items.iter().zip(components.iter()) {
                bind_match_pattern(item, ty, enums, env);
            }
        }
        // A variant with no known payload types, or literals/ranges/wildcards
        // that bind nothing; an `Or` binds the same names in each branch, so
        // the first suffices.
        Pattern::Or(branches) => {
            if let Some(first) = branches.first() {
                bind_match_pattern(first, scrutinee, enums, env);
            }
        }
        _ => {}
    }
}

/// Bind an arm's components and check that its pattern is a shape of the type
/// the `select` consumes.
fn bind_select_arm(
    consumed: &Type,
    pattern: &slc_syntax::ast::Pattern,
    declarations: &Declarations,
    env: &mut Env,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use slc_syntax::ast::Pattern;
    // A name that is not a variant binds the whole value: a type with no
    // structure has one shape, whose single component is the value itself.
    if let Pattern::Ident(name) = pattern
        && declarations.variant(name).is_none()
    {
        env.define(name, consumed.clone());
        return;
    }
    if matches!(pattern, Pattern::Wildcard) {
        return;
    }
    let components: Vec<Type> = match (consumed, pattern) {
        // An enum variant: its payload types.
        (Type::Named(_), Pattern::Ident(_) | Pattern::Enum { .. }) => {
            let written = match pattern {
                Pattern::Ident(name) => name.clone(),
                Pattern::Enum { name, variant, .. } => {
                    if variant.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}::{variant}")
                    }
                }
                _ => unreachable!("matched above"),
            };
            match declarations.variant(&written) {
                Some((_, payload)) => payload.clone(),
                None => {
                    diags.push(Diagnostic {
                        message: format!("`{written}` is not a variant of {consumed}"),
                        span,
                    });
                    return;
                }
            }
        }
        // A struct: its field types.
        (Type::Named(name), Pattern::Struct { name: written, .. }) => {
            if written != name {
                diags.push(Diagnostic {
                    message: format!("`select {consumed}` arm cannot bind a `{written}`"),
                    span,
                });
                return;
            }
            declarations.fields(name).unwrap_or_default()
        }
        // A tensor: its components, flattened right-nested.
        (Type::Tensor(..), Pattern::Tuple(_)) => flatten_tensor(consumed),
        _ => {
            diags.push(Diagnostic {
                message: format!("a `select {consumed}` arm must cover a shape of {consumed}"),
                span,
            });
            return;
        }
    };

    let binders: Vec<&slc_syntax::ast::Pattern> = match pattern {
        Pattern::Enum { fields, .. } => fields.iter().collect(),
        Pattern::Struct { fields, .. } => fields.iter().map(|(_, p)| p).collect(),
        Pattern::Tuple(items) => items.iter().collect(),
        _ => Vec::new(),
    };
    if binders.len() != components.len() {
        diags.push(Diagnostic {
            message: format!(
                "this shape has {} component(s); the arm binds {}",
                components.len(),
                binders.len()
            ),
            span,
        });
    }
    for (binder, ty) in binders.iter().zip(components) {
        if let Pattern::Ident(name) = binder {
            env.define(name, ty);
        }
    }
}

/// The components of a right-nested tensor.
fn flatten_tensor(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Tensor(head, rest) => {
            let mut out = vec![(**head).clone()];
            out.extend(flatten_tensor(rest));
            out
        }
        other => vec![other.clone()],
    }
}

fn infer_expr(
    e: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    check_expr(e, enums, env, diags)
}

/// Check an expression and give back its type, solved as far as unification
/// currently knows — a caller never sees a variable that already has an
/// answer.
fn check_expr(
    e: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    let found = check_expr_unapplied(e, enums, env, diags);
    found.map(|ty| env.uni.apply(&ty))
}

fn check_expr_unapplied(
    e: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    if let Some(ty) = literal_type(&e.kind) {
        return Some(ty);
    }
    match &e.kind {
        Expr::Ident(name) => {
            if let Some(ty) = env.lookup_instantiated(name) {
                return Some(ty);
            }
            if let Some(ty) = env.lookup(name) {
                return Some(ty);
            }
            // The global consumer is gone: ending the program is a right a
            // helper is handed, not one it takes.
            if name == "EXIT" {
                diags.push(Diagnostic {
                    message: "the top-level `EXIT` no longer exists; end the program through \
                              a continuation parameter, the way `main` does with `exit`"
                        .into(),
                    span: e.span,
                });
                return None;
            }
            if enums.declarations.contains(name) {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` is a declaration name, not a value; write a variant or a \
                         literal of it"
                    ),
                    span: e.span,
                });
                return None;
            }
            let (declaration, payload) = enums.variant(name)?;
            if !payload.is_empty() {
                diags.push(Diagnostic {
                    message: format!(
                        "variant `{name}` carries {} payload value(s); it is a value only when \
                         applied to them",
                        payload.len()
                    ),
                    span: e.span,
                });
                return None;
            }
            Some(Type::Named(declaration.clone()))
        }
        Expr::Lambda { param, param_type, body, .. } => {
            env.push();
            let param_ty = param_type
                .as_ref()
                .and_then(|ty| enums.resolve(ty))
                .or_else(|| infer_param_type(param, body, enums, env))
                .unwrap_or_else(|| env.uni.fresh_var());
            env.define(param, param_ty.clone());
            let result = check_expr(body, enums, env, diags);
            env.pop();
            // A lambda is a function value, and its result is what the body
            // produces. A body that ends in a cut produces nothing, and
            // `A → ⊥` is `-A`, so such a lambda simply *is* a consumer.
            let result = result.unwrap_or_else(|| env.uni.fresh_var());
            Some(Type::arrow(param_ty, result))
        }
        Expr::Call { callee, args } => {
            // A variant applied to its payload is a value, not a call.
            if let Expr::Ident(name) = &callee.kind
                && env.lookup(name).is_none()
                && let Some((declaration, payload)) = enums.variant(name)
            {
                let declaration = declaration.clone();
                let payload = payload.clone();
                if args.len() != payload.len() {
                    diags.push(Diagnostic {
                        message: format!(
                            "variant `{name}` carries {} payload value(s); the expression \
                             supplies {}",
                            payload.len(),
                            args.len()
                        ),
                        span: e.span,
                    });
                }
                for (arg, expected) in args.iter().zip(payload.iter()) {
                    if let Some(actual) = check_expr(arg, enums, env, diags)
                        && !fits(env, expected, &actual, &arg.kind)
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "payload of `{name}` has type {actual}; the variant declares \
                                 {expected}"
                            ),
                            span: arg.span,
                        });
                    }
                }
                return Some(Type::Named(declaration));
            }
            // A continuation is not applied: it is cut against a value. Only
            // an atomic consumer is certainly not a function — `A → B` is
            // `-A ⅋ B`, so a function is negative too, and a `⅋` may be
            // either a function or a consumer of a product.
            // Nor is data applied: a `+A` is a value, and a value is not a
            // function.
            if let Expr::Ident(name) = &callee.kind
                && let Some(ty) = env.lookup(name)
                && matches!(ty, Type::Pos(_))
            {
                diags.push(Diagnostic {
                    message: format!("`{name}` has type {ty}, which is not a function"),
                    span: e.span,
                });
                return None;
            }
            if let Expr::Ident(name) = &callee.kind
                && let Some(ty) = env.lookup(name)
                && matches!(ty, Type::Neg(_) | Type::Bottom | Type::Dual(_))
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` is a consumer of type {ty}, not a function; send it a value \
                         with a cut: `value @ {name}`"
                    ),
                    span: e.span,
                });
                for arg in args {
                    check_expr(arg, enums, env, diags);
                }
                return Some(Type::Bottom);
            }
            // A trait method dispatches on its first argument; type it
            // against the method signature and discharge the bound.
            if let Expr::Ident(name) = &callee.kind
                && env.traits.is_method(name)
            {
                return check_trait_method_call(name, args, e.span, enums, env, diags);
            }
            let callee_ty = check_expr(callee, enums, env, diags);
            if let Expr::Ident(name) = &callee.kind
                && let Some(signature) = env.functions.get(name)
            {
                let (signature, seen) = instantiate(signature, &mut env.uni);
                if is_builtin(name) {
                    for ((arg, param), is_continuation) in
                        args.iter().zip(signature.params.iter()).zip(&signature.continuations)
                    {
                        if *is_continuation {
                            continue;
                        }
                        if let Some(actual) = check_expr(arg, enums, env, diags)
                            && !fits(env, param, &actual, &arg.kind)
                        {
                            let param = env.uni.apply(param);
                            diags.push(Diagnostic {
                                message: format!(
                                    "argument to `{name}` has type {actual}; expected {param}"
                                ),
                                span: arg.span,
                            });
                        }
                    }
                }
                check_call_arguments(name, &signature, args, enums, env, diags);
                // Discharge each bound against what its type parameter
                // resolved to, now that the arguments have constrained it, and
                // record the dictionary the call must pass for it: the global
                // dict of a concrete type, or the enclosing function's own
                // dict parameter when the bound is forwarded.
                let mut dict_args: Vec<String> = Vec::new();
                for (param_index, trait_name) in &signature.bounds {
                    if let Some(var) = seen.get(param_index) {
                        let target = env.uni.apply(var);
                        discharge_bound(trait_name, &target, name, e.span, env, diags);
                        let dict = match &target {
                            Type::Var(v) => env
                                .bounds
                                .iter()
                                .find(|(bv, bt, _)| bv == v && bt == trait_name)
                                .map(|(_, _, tp)| {
                                    slc_syntax::lower::dict_param_name(trait_name, tp)
                                }),
                            _ => type_key(&target)
                                .filter(|key| env.traits.has_impl(trait_name, key))
                                .map(|key| slc_syntax::lower::dict_global_name(trait_name, &key)),
                        };
                        if let Some(dict) = dict {
                            dict_args.push(dict);
                        }
                    }
                }
                if !signature.bounds.is_empty() {
                    env.dispatch.calls.insert(e.span, dict_args);
                }
                return signature.result.map(|ty| env.uni.apply(&ty));
            }
            // A local callee: a closure, or a binder whose type its uses
            // decide. `A → B` is `-A ⅋ B`, so application peels a `⅋`, and
            // an unknown callee becomes one.
            let callee_ty = callee_ty.map(|ty| env.uni.apply(&ty));
            match callee_ty {
                Some(Type::Par(argument_dual, result)) => {
                    if let [argument] = args.as_slice()
                        && let Some(actual) = check_expr(argument, enums, env, diags)
                        && !fits(env, &argument_dual.dual(), &actual, &argument.kind)
                    {
                        let expected = env.uni.apply(&argument_dual.dual());
                        diags.push(Diagnostic {
                            message: format!(
                                "this call's argument has type {actual}; the function takes \
                                 {expected}"
                            ),
                            span: argument.span,
                        });
                    }
                    Some(env.uni.apply(&result))
                }
                Some(Type::Var(_)) if args.len() == 1 => {
                    let actual = check_expr(&args[0], enums, env, diags)
                        .unwrap_or_else(|| env.uni.fresh_var());
                    let result = env.uni.fresh_var();
                    let callee_ty = callee_ty.unwrap();
                    if env.uni.unify(&callee_ty, &Type::arrow(actual, result.clone())).is_err() {
                        diags.push(Diagnostic {
                            message: format!(
                                "this callee has type {}, which is not a function",
                                env.uni.apply(&callee_ty)
                            ),
                            span: callee.span,
                        });
                        return None;
                    }
                    Some(env.uni.apply(&result))
                }
                _ => {
                    for arg in args {
                        check_expr(arg, enums, env, diags);
                    }
                    None
                }
            }
        }
        Expr::If { cond, then, otherwise } => {
            let cond_ty = check_expr(cond, enums, env, diags);
            if !cond_ty.as_ref().is_some_and(|ty| fits(env, &Type::Pos(Base::Bool), ty, &cond.kind))
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`if` condition has type {}; expected +bool",
                        cond_ty.map(|ty| ty.to_string()).unwrap_or_else(|| "unknown".into())
                    ),
                    span: cond.span,
                });
            }
            let then_ty = check_expr(then, enums, env, diags);
            let Some(otherwise) = otherwise else {
                return then_ty;
            };
            let else_ty = check_expr(otherwise, enums, env, diags);
            // A branch that ends in a cut never returns, so it constrains
            // nothing: the `if` has the type of the branch that does return.
            match (then_ty, else_ty) {
                (Some(Type::Bottom), other) | (other, Some(Type::Bottom)) => other,
                (Some(then_ty), Some(else_ty)) => {
                    if env.uni.unify(&then_ty, &else_ty).is_err() {
                        let then_ty = env.uni.apply(&then_ty);
                        let else_ty = env.uni.apply(&else_ty);
                        diags.push(Diagnostic {
                            message: format!(
                                "`if` branches have incompatible types {then_ty} and {else_ty}"
                            ),
                            span: otherwise.span,
                        });
                    }
                    Some(then_ty)
                }
                (then_ty, _) => then_ty,
            }
        }
        Expr::Let { name, ty, value, body } => {
            let binding_ty = check_let_binding(name, ty, value, enums, env, diags);
            let (binding_ty, generalized) = generalize(binding_ty, &value.kind, enums, env);
            env.push();
            env.define_scheme(name, binding_ty, generalized);
            let result = body.as_ref().and_then(|body| check_expr(body, enums, env, diags));
            env.pop();
            result
        }
        Expr::BinOp { op, lhs, rhs } => {
            let lhs_ty = check_expr(lhs, enums, env, diags);
            let rhs_ty = check_expr(rhs, enums, env, diags);
            match op {
                slc_syntax::ast::BinOp::And | slc_syntax::ast::BinOp::Or => {
                    for (operand, ty) in [(lhs, lhs_ty.clone()), (rhs, rhs_ty.clone())] {
                        if !ty
                            .as_ref()
                            .is_some_and(|ty| fits(env, &Type::Pos(Base::Bool), ty, &operand.kind))
                        {
                            diags.push(Diagnostic {
                                message: format!(
                                    "boolean operand has type {}; expected +bool",
                                    ty.map(|ty| ty.to_string()).unwrap_or_else(|| "unknown".into())
                                ),
                                span: operand.span,
                            });
                        }
                    }
                    Some(Type::Pos(Base::Bool))
                }
                slc_syntax::ast::BinOp::Add
                | slc_syntax::ast::BinOp::Sub
                | slc_syntax::ast::BinOp::Mul
                | slc_syntax::ast::BinOp::Div
                | slc_syntax::ast::BinOp::Mod => {
                    if let (Some(lhs_ty), Some(rhs_ty)) = (lhs_ty.clone(), rhs_ty.clone()) {
                        // The operands agree — an integer literal adapting
                        // to the other side's width.
                        let agree = fits(env, &lhs_ty, &rhs_ty, &rhs.kind)
                            || fits(env, &rhs_ty, &lhs_ty, &lhs.kind);
                        let mut joined = env.uni.apply(&lhs_ty);
                        // An operand nothing constrains is the default
                        // integer, exactly as a bare literal is.
                        if agree && matches!(joined, Type::Var(_)) {
                            let _ = env.uni.unify(&joined, &Type::Pos(Base::I64));
                            joined = env.uni.apply(&joined);
                        }
                        let string_add = agree
                            && matches!(op, slc_syntax::ast::BinOp::Add)
                            && joined == Type::Pos(Base::Str);
                        if string_add {
                            return Some(Type::Pos(Base::Str));
                        }
                        if !agree || !is_numeric(&joined) {
                            let lhs_ty = env.uni.apply(&lhs_ty);
                            let rhs_ty = env.uni.apply(&rhs_ty);
                            diags.push(Diagnostic {
                                message: format!(
                                    "arithmetic operands have types {lhs_ty} and {rhs_ty}"
                                ),
                                span: e.span,
                            });
                        }
                        return Some(joined);
                    }
                    lhs_ty.or(rhs_ty)
                }
                slc_syntax::ast::BinOp::Eq
                | slc_syntax::ast::BinOp::Ne
                | slc_syntax::ast::BinOp::Lt
                | slc_syntax::ast::BinOp::Gt
                | slc_syntax::ast::BinOp::Le
                | slc_syntax::ast::BinOp::Ge => {
                    if let (Some(lhs_ty), Some(rhs_ty)) = (lhs_ty, rhs_ty)
                        && !{
                            let agree = fits(env, &lhs_ty, &rhs_ty, &rhs.kind)
                                || fits(env, &rhs_ty, &lhs_ty, &lhs.kind);
                            let joined = env.uni.apply(&lhs_ty);
                            agree && (matches!(joined, Type::Var(_)) || is_comparable(&joined))
                        }
                    {
                        let lhs_ty = env.uni.apply(&lhs_ty);
                        let rhs_ty = env.uni.apply(&rhs_ty);
                        diags.push(Diagnostic {
                            message: format!(
                                "comparison operands have types {lhs_ty} and {rhs_ty}"
                            ),
                            span: e.span,
                        });
                    }
                    Some(Type::Pos(Base::Bool))
                }
            }
        }
        Expr::UnOp { op, body } => {
            let body_ty = check_expr(body, enums, env, diags);
            match op {
                slc_syntax::ast::UnOp::Not => {
                    if body_ty != Some(Type::Pos(Base::Bool)) {
                        diags.push(Diagnostic {
                            message: format!(
                                "`!` operand has type {}; expected +bool",
                                body_ty
                                    .map(|ty| ty.to_string())
                                    .unwrap_or_else(|| "unknown".into())
                            ),
                            span: body.span,
                        });
                    }
                    Some(Type::Pos(Base::Bool))
                }
                slc_syntax::ast::UnOp::Neg => {
                    if body_ty.as_ref().is_some_and(|ty| !is_numeric(ty)) {
                        diags.push(Diagnostic {
                            message: format!(
                                "unary `-` operand has type {body_ty:?}; expected numeric"
                            ),
                            span: body.span,
                        });
                    }
                    body_ty
                }
            }
        }
        Expr::Index { value, index } => {
            let value_ty = check_expr(value, enums, env, diags);
            let index_ty = check_expr(index, enums, env, diags);
            if let Some(value_ty) = value_ty.clone() {
                match expected_index_type(&value_ty) {
                    Some(expected_index) => {
                        if index_ty != Some(expected_index.clone()) {
                            diags.push(Diagnostic {
                                message: format!(
                                    "index has type {}; expected {expected_index}",
                                    index_ty
                                        .map(|ty| ty.to_string())
                                        .unwrap_or_else(|| "unknown".into())
                                ),
                                span: index.span,
                            });
                        }
                    }
                    None => {
                        diags.push(Diagnostic {
                            message: format!("type {value_ty} is not indexable"),
                            span: value.span,
                        });
                    }
                }
                index_result_type(&value_ty)
            } else {
                None
            }
        }
        Expr::Slice { value, start, end } => {
            let value_ty = check_expr(value, enums, env, diags);
            for endpoint in [start, end].into_iter().flatten() {
                let endpoint_ty = check_expr(endpoint, enums, env, diags);
                if endpoint_ty != Some(Type::Pos(Base::I64)) {
                    diags.push(Diagnostic {
                        message: format!(
                            "range endpoint has type {}; expected +i64",
                            endpoint_ty
                                .map(|ty| ty.to_string())
                                .unwrap_or_else(|| "unknown".into())
                        ),
                        span: endpoint.span,
                    });
                }
            }
            if let Some(Type::List(inner)) = value_ty.clone() {
                Some(Type::List(inner))
            } else {
                value_ty
            }
        }
        Expr::Match { scrutinee, arms } => {
            let scrutinee_ty = check_expr(scrutinee, enums, env, diags);
            if let Some(scrutinee_ty) = &scrutinee_ty {
                for arm in arms {
                    check_pattern(&arm.pattern, scrutinee_ty, enums, arm.body.span, diags);
                }
            }
            for arm in arms {
                env.push();
                // Bind the arm's pattern variables with their declared types,
                // so the body sees `h: i64` for `Cons(h, _)`.
                if let Some(scrutinee_ty) = &scrutinee_ty {
                    bind_match_pattern(&arm.pattern, scrutinee_ty, enums, env);
                }
                if let Some(guard) = &arm.guard {
                    let guard_ty = check_expr(guard, enums, env, diags);
                    if guard_ty != Some(Type::Pos(Base::Bool)) {
                        diags.push(Diagnostic {
                            message: format!(
                                "match guard has type {}; expected +bool",
                                guard_ty
                                    .map(|ty| ty.to_string())
                                    .unwrap_or_else(|| "unknown".into())
                            ),
                            span: guard.span,
                        });
                    }
                }
                check_expr(&arm.body, enums, env, diags);
                env.pop();
            }
            None
        }
        Expr::Struct { name, fields } => {
            let Some(declared) = enums.structs.get(name).cloned() else {
                diags.push(Diagnostic {
                    message: format!("`{name}` is not a declared struct"),
                    span: e.span,
                });
                for (_, value) in fields {
                    check_expr(value, enums, env, diags);
                }
                return None;
            };
            // A struct literal is the product of its declared fields: every
            // field is present exactly once, in declaration order, with the
            // declared type.
            let written: Vec<&String> = fields.iter().map(|(field, _)| field).collect();
            let expected: Vec<&String> = declared.iter().map(|(field, _)| field).collect();
            if written != expected {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` has fields {}; the literal writes {}",
                        expected.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", "),
                        if written.is_empty() {
                            "none".to_string()
                        } else {
                            written.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
                        }
                    ),
                    span: e.span,
                });
            }
            for (field, value) in fields {
                let actual = check_expr(value, enums, env, diags);
                if let (Some(actual), Some((_, expected))) =
                    (actual, declared.iter().find(|(declared, _)| declared == field))
                    && !fits(env, expected, &actual, &value.kind)
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "field `{field}` of `{name}` has type {actual}; the declaration \
                             says {expected}"
                        ),
                        span: value.span,
                    });
                }
            }
            Some(Type::Named(name.clone()))
        }
        Expr::Select { ty, arms } => {
            // `select T { p <= c, … }` builds the consumer of T. Each arm
            // covers one shape of T, binds that shape's components, and runs
            // a command; the whole expression is dual to T.
            let resolved = match ty {
                Some(ty) => match enums.resolve(&ty.kind) {
                    Some(resolved) => Some(resolved),
                    None => {
                        diags.push(Diagnostic {
                            message: "`select` needs a declared type or an explicit connective"
                                .into(),
                            span: ty.span,
                        });
                        return None;
                    }
                },
                // Left out: an arm's pattern may name the type, and inside a
                // negative `fn` the declaration already said it.
                None => named_by_arms(arms, enums).or_else(|| env.consumed.clone()),
            };
            let Some(resolved) = resolved else {
                diags.push(Diagnostic {
                    message: "no arm names a type, so write what this `select` consumes".into(),
                    span: e.span,
                });
                return None;
            };
            if resolved.is_negative() {
                diags.push(Diagnostic {
                    message: format!(
                        "`select` consumes data, and {resolved} is a consumer; a consumer is \
                         consumed in a box: `select ↓{resolved}`"
                    ),
                    span: e.span,
                });
                return None;
            }
            for arm in arms {
                env.push();
                bind_select_arm(&resolved, &arm.pattern, enums, env, e.span, diags);
                let command = check_expr(&arm.command, enums, env, diags);
                if let Some(command) = command
                    && command != Type::Bottom
                    && command != Type::One
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "a `select` arm is a command; this one has type {command}"
                        ),
                        span: arm.command.span,
                    });
                }
                env.pop();
            }
            Some(resolved.dual())
        }
        // `↓e` boxes a consumer as data; `↑e` opens the box. Neither does
        // anything at run time — they are here so that a value and a
        // suspended computation are not the same type.
        Expr::Shift { down: true, expr } => {
            let inner = check_expr(expr, enums, env, diags)?;
            if !inner.is_negative() {
                diags.push(Diagnostic {
                    message: format!("`↓` boxes a consumer; this has type {inner}"),
                    span: e.span,
                });
                return None;
            }
            Some(Type::Down(Box::new(inner)))
        }
        Expr::Shift { down: false, expr } => {
            let inner = check_expr(expr, enums, env, diags)?;
            match inner {
                Type::Down(boxed) => Some(*boxed),
                other => {
                    diags.push(Diagnostic {
                        message: format!("`↑` opens a `↓` box; this has type {other}"),
                        span: e.span,
                    });
                    None
                }
            }
        }
        Expr::Handle { body, clauses, ret, .. } => {
            // The body runs under the handler; its normal value feeds the
            // return clause, whose body is the handle's type. (Effect rows
            // are not yet checked — see PLAN.)
            let body_ty =
                check_expr(body, enums, env, diags).unwrap_or_else(|| env.uni.fresh_var());
            for clause in clauses {
                env.push();
                for p in &clause.params {
                    let v = env.uni.fresh_var();
                    env.define(p, v);
                }
                let resume_in = env.uni.fresh_var();
                let resume_out = env.uni.fresh_var();
                env.define(&clause.resume, Type::arrow(resume_in, resume_out));
                check_expr(&clause.body, enums, env, diags);
                env.pop();
            }
            match ret {
                Some((binder, rbody)) => {
                    env.push();
                    env.define(binder, body_ty);
                    let ty = check_expr(rbody, enums, env, diags);
                    env.pop();
                    ty
                }
                None => Some(body_ty),
            }
        }
        Expr::Pair(items) if items.is_empty() => Some(Type::One),
        Expr::Pair(items) => items
            .iter()
            .map(|item| check_expr(item, enums, env, diags))
            .collect::<Option<Vec<_>>>()
            .map(|types| {
                types.into_iter().rev().reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
            })?,
        Expr::Block(exprs) => {
            let mut result = None;
            env.push();
            for expr in exprs {
                if let Expr::Let { name, ty, value, body: None } = &expr.kind {
                    // A bodyless `let` scopes over the rest of the block; the
                    // binding itself is checked exactly as the expression form.
                    let binding_ty = check_let_binding(name, ty, value, enums, env, diags);
                    let (binding_ty, generalized) = generalize(binding_ty, &value.kind, enums, env);
                    env.define_scheme(name, binding_ty, generalized);
                } else {
                    result = check_expr(expr, enums, env, diags);
                }
            }
            env.pop();
            result
        }
        Expr::Cut { value, consumer } => {
            // `v @ k` is a command: it sends `v` to the consumer `k` and does
            // not return, so its type is bottom. A cut is well typed when the
            // two sides are dual — that is what makes the interaction fit,
            // and with `A → B` being `-A ⅋ B` it is the whole rule: which
            // side is written negatively is not itself the question.
            let value_ty = check_expr(value, enums, env, diags);
            let consumer_ty = check_expr(consumer, enums, env, diags);
            // The value side carries data. A consumer travels only in a box:
            // `↓k @ …`, never `k @ …` — without this rule, `dual` being an
            // involution would let a consumer of consumers pass for the data
            // it consumes.
            if let Some(value_ty) = &value_ty
                && value_ty.is_negative()
                // An unsolved variable is not yet anything; the duality
                // check below still constrains it.
                && !contains_var(value_ty)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "the left of `@` is data, and this has type {value_ty}; a consumer is \
                         sent in a box: `↓v @ …`"
                    ),
                    span: value.span,
                });
                return Some(Type::Bottom);
            }
            if let (Some(value_ty), Some(consumer_ty)) = (&value_ty, &consumer_ty)
                // The ⊥/1 corner: `-⊥` resolves to `1`, so the idiomatic
                // `() @ k` against it is unit meeting unit.
                && !(value_ty == &Type::One && consumer_ty == &Type::One)
                && !fits(env, &consumer_ty.dual(), value_ty, &value.kind)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`@` sends a value to a consumer of it: {consumer_ty} accepts {}, \
                         and the value has type {value_ty}",
                        consumer_ty.dual()
                    ),
                    span: e.span,
                });
            }
            Some(Type::Bottom)
        }
        Expr::Mu { continuation_params, body, .. } => {
            env.push();
            let mut captured_types = Vec::new();
            for p in continuation_params {
                let ty = match &p.ty {
                    Some(ty) => enums.resolve(ty),
                    // Nothing was written, so the body says it.
                    None => infer_param_type(&p.name, body, enums, env),
                }
                .unwrap_or_else(|| env.uni.fresh_var());
                env.define(&p.name, ty.clone());
                captured_types.push(Some(ty));
            }
            let result = check_expr(body, enums, env, diags);
            env.pop();
            // A `mu` captures the ambient continuation, so its value is
            // whatever that continuation receives: `mu(k: -A) { … }` has
            // type `A`. A row whose positions disagree has no single such
            // type, and neither does one that is empty.
            let captured = captured_types
                .into_iter()
                .map(|ty| ty.map(|ty| ty.dual()))
                .collect::<Option<Vec<_>>>()
                .filter(|types| {
                    !types.is_empty() && types.windows(2).all(|pair| pair[0] == pair[1])
                })
                .map(|mut types| types.remove(0));
            captured.or(result)
        }
        _ => None,
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
        let (prog, traits) = slc_syntax::traits::elaborate(&prog).expect("elaborate");
        check_program(&prog, &traits)
    }

    #[test]
    fn struct_literal_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "struct Direction { left: i64, right: i64 }
                 fn f() -> i64 { use_it(Direction { left: 1, right: 2 }) }"
            )
            .is_ok()
        );

        let missing = check(
            "struct Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            missing.iter().any(|d| d.message.contains("the literal writes `left`")),
            "{missing:?}"
        );

        let reordered = check(
            "struct Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { right: 2, left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            reordered.iter().any(|d| d.message.contains("the literal writes `right`, `left`")),
            "{reordered:?}"
        );
    }

    #[test]
    fn struct_pattern_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "struct D { left: i64, right: i64 }
                 fn f(d: D) -> i64 {
                     match d {
                         D { left: a, right: b } => a,
                         _ => 0,
                     }
                 }"
            )
            .is_ok()
        );

        let diags = check(
            "struct D { left: i64, right: i64 }
             fn f(d: D) -> i64 {
                 match d {
                     D { right: b, left: a } => a,
                     _ => 0,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("the pattern writes `right`, `left`")),
            "{diags:?}"
        );
    }

    #[test]
    fn struct_pattern_field_types_are_checked() {
        let diags = check(
            "struct D { left: i64, right: i64 }
             fn f(d: D) -> i64 {
                 match d {
                     D { left: 1, right: 'c' } => 0,
                     _ => 0,
                 }
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type +char")), "{diags:?}");
    }

    #[test]
    fn struct_pattern_cannot_match_another_type() {
        let diags = check(
            "struct D { left: i64 }
             fn f(x: +i64) -> i64 {
                 match x {
                     D { left: a } => a,
                     _ => 0,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("cannot match a scrutinee of type +i64")),
            "{diags:?}"
        );
    }

    #[test]
    fn struct_literal_field_types_are_checked() {
        let diags = check(
            "struct Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { left: 1, right: \"two\" }) }",
        )
        .unwrap_err();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("field `right` of `Direction` has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn undeclared_struct_literal_is_rejected() {
        let diags = check("fn f() -> i64 { use_it(Nope { a: 1 }) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`Nope` is not a declared struct")),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_variant_without_payload_is_a_value() {
        assert!(check("enum Color { Red } fn f() -> Color { Color::Red }").is_ok());
        assert!(check("enum Color { Red } fn f() -> Color { Red }").is_ok());
    }

    #[test]
    fn enum_variant_with_payload_is_a_value_only_when_applied() {
        // An integer literal has type `+i64`.
        assert!(check("enum R { Some(i64) } fn f() -> R { R::Some(1) }").is_ok());
        let diags = check("enum R { Some(i64) } fn f() -> R { R::Some }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("carries 1 payload value(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_variant_payload_arity_is_checked() {
        let diags = check("enum R { Both(i64, i64) } fn f() -> R { R::Both(1) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the expression supplies 1")), "{diags:?}");
    }

    #[test]
    fn enum_variant_payload_type_is_checked() {
        let diags = check("enum R { Some(i64) } fn f() -> R { R::Some(\"text\") }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("payload of `R::Some` has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn declaration_name_is_not_a_value() {
        for source in [
            "enum Color { Red } fn f() -> Color { Color }",
            "struct S { a: i32 } fn f() -> i32 { S }",
        ] {
            let diags = check(source).unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains("is a declaration name, not a value")),
                "{source}: {diags:?}"
            );
        }
    }

    #[test]
    fn select_arm_binds_the_variant_payload() {
        assert!(
            check(
                "enum R { Some(i64), None }
                 fn k(ok: -i64, absent: -i64) <- R {
                     select R {
                         Some(value) <= value @ ok,
                         None <= 0 @ absent,
                     }
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn select_arm_must_bind_a_declared_payload() {
        let diags = check(
            "enum R { Some(i64), None }
             fn k(ok: -i64, absent: -i64) <- R {
                 select R {
                     Some <= 0 @ ok,
                     None <= 0 @ absent,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("this shape has 1 component(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn select_arm_cannot_bind_a_payload_a_variant_does_not_have() {
        let diags = check(
            "enum R { None }
             fn k(absent: -i32) <- R {
                 select R {
                     None(value) <= 0 @ absent,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("this shape has 0 component(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_continuation_is_cut_against_not_called() {
        let diags = check("command route(x: +i32) | (k: -i32) { k(x) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("is a consumer of type -i32, not a function")
                && d.message.contains("value @ k")),
            "{diags:?}"
        );
        assert!(check("command route(x: +i32) | (k: -i32) { x @ k }").is_ok());
    }

    #[test]
    fn exit_is_a_continuation() {
        let diags = check("command f | (out: -i32) { out(0) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`out` is a consumer of type -i32")),
            "{diags:?}"
        );
        assert!(check("command f | (out: -i32) { 0 @ out }").is_ok());
    }

    #[test]
    fn a_cut_is_a_command() {
        // A cut has type ⊥: it produces nothing and control does not return,
        // so a branch that ends in one leaves the `if` type to the other.
        let ok = check(
            "fn parse(input: +String, err: -String) -> i64 {
                 if str_len(input) > 0 { 1 } else { \"empty\" @ err }
             }",
        );
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn a_cut_needs_dual_sides() {
        // Two values of the same positive type do not interact.
        let diags = check("fn f(x: +i32, y: +i32) -> i32 { x @ y }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("sends a value to a consumer of it")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_cut_checks_what_the_consumer_accepts() {
        let diags = check("command route(x: +String) | (k: -i32) { x @ k }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("-i32 accepts +i32")
                && d.message.contains("the value has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn applying_a_negative_function_to_a_continuation_is_application() {
        // Supplying a continuation to a negative function is an ordinary
        // call: the declared row says the argument is a consumer.
        assert!(
            check(
                "enum Color { Red, Green }
                 fn code(return: -i64) <- Color {
                     select Color {
                         Red <= 0 @ return,
                         Green <= 1 @ return,
                     }
                 }
                 fn main() -> i64 {
                     mu ask(answer: -i64) { Color::Green @ code(answer) }
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_accepts_the_declared_row() {
        assert!(
            check(
                "command route(x: +i32) | (k: -i32) { x @ k }
                 fn main() -> i32 { mu run(out: -i32) { route(1, out) } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_rejects_an_incompatible_continuation_type() {
        let diags = check(
            "command route(x: +i32) | (k: -i32) { x @ k }
             fn main() -> i32 { mu run(out: -bool) { route(1, out) } }",
        )
        .unwrap_err();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("continuation row mismatch")
                    && d.message.contains("-bool")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn continuation_row_is_positional() {
        // The row is ordered: swapping two continuations of different types
        // is rejected even though both types appear in the declaration.
        let diags = check(
            "command route(a: -i32, b: -bool) | (c: -i32, d: -bool) { 0 @ c }
             fn main() -> i32 {
                 mu run(first: -i32, second: -bool) { route(0, true, second, first) }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().filter(|d| d.message.contains("continuation row mismatch")).count() == 2,
            "both swapped positions should be rejected: {diags:?}"
        );
    }

    #[test]
    fn continuation_row_rejects_extra_arguments() {
        let diags = check(
            "command route(x: +i32) | (k: -i32) { x @ k }
             fn main() -> i32 { mu run(out: -i32) { route(1, out, out) } }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("continuation row of 1")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn boolean_operators_ok() {
        assert!(check("fn f(a: +bool, b: +bool) -> bool { a && b || !a }").is_ok());
    }

    #[test]
    fn boolean_operand_mismatch_rejected() {
        let diags = check("fn f(a: +i32) -> bool { a && true }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("boolean operand")));
        assert!(diags[0].span.start > 0);
    }

    #[test]
    fn arithmetic_type_mismatch_rejected() {
        let diags = check("fn f(a: +i32, b: +i64) -> i64 { a + b }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("arithmetic operands")));
    }

    #[test]
    fn comparison_char_and_int_mismatch_rejected() {
        let diags = check("fn f(a: +char, b: +i32) -> bool { a < b }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("comparison operands")));
    }

    #[test]
    fn char_comparison_ok() {
        assert!(check("fn f(a: +char, b: +char) -> bool { a < b }").is_ok());
    }

    #[test]
    fn string_concat_ok() {
        assert!(check(r#"fn f(a: +String, b: +String) -> String { a + b }"#).is_ok());
    }

    #[test]
    fn guard_must_be_bool() {
        let diags = check("fn f(c: +i64) -> i64 { match c { _ if c => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("match guard")));
    }

    #[test]
    fn char_pattern_checked() {
        let diags = check("fn f(c: +char) -> i64 { match c { 3 => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type")));
    }

    #[test]
    fn index_type_checked() {
        let diags = check("fn f(s: +String, i: +bool) -> char { s[i] }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("index has type")));
    }

    #[test]
    fn list_index_checked() {
        assert!(check("fn f(l: [+i32], i: +i64) -> i32 { l[i] }").is_ok());
        let diags = check("fn f(l: [+i32], i: +bool) -> i32 { l[i] }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("index has type")));
    }

    #[test]
    fn typed_let_checked() {
        let diags = check("fn f() -> i32 { let x: +char = 1; 2 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("annotated")));
    }

    #[test]
    fn const_type_checked() {
        let diags = check("const X: +char = 1;").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("initializer")));
    }

    #[test]
    fn non_constant_const_initializer_rejected() {
        let diags = check("const X: +i32 = add(1, 2);").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("literal or another constant")));
    }

    #[test]
    fn range_endpoints_checked() {
        let diags = check("fn f(c: +char) -> i64 { match c { 'a'..=3 => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type")));
    }

    #[test]
    fn select_expression_has_dual_named_type() {
        let r = check(
            "enum Color { Red, Green, Blue }
            fn k(return: -i32) <- Color {
                select Color {
                    Red <= 0 @ return,
                    Green <= 1 @ return,
                    Blue <= 2 @ return,
                }
            }",
        );
        assert!(r.is_ok(), "unexpected diagnostics: {r:?}");
    }

    #[test]
    fn select_arm_must_be_a_command() {
        let diags = check(
            "enum Color { Red, Green }
            fn k(return: -i32) <- Color {
                select Color {
                    Red <= 0,
                    Green <= 1 @ return,
                }
            }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`select` arm is a command")), "{diags:?}");
    }

    #[test]
    fn select_arm_must_cover_a_shape_of_the_type() {
        let diags = check(
            "enum Color { Red, Green }
            fn k(return: -i32) <- Color {
                select Color {
                    (a, b) <= 0 @ return,
                    Green <= 1 @ return,
                }
            }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must cover a shape of")), "{diags:?}");
    }

    #[test]
    fn unit_is_a_type_and_not_a_wildcard() {
        let diags = check(
            "fn wants(x: +String) -> i64 { 0 }
             command main | (exit: -i32) { println(wants(())); 0 @ exit }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type 1")),
            "unit fit everything once: {diags:?}"
        );
    }

    #[test]
    fn a_handler_may_resume_once_in_any_position() {
        // Work after a single resume is fine now — the continuation is one
        // stack, so the resumed result flows back into the clause.
        assert!(
            check(
                "effect Ask { fn ask() -> i64; }
                 fn u() -> i64 / {Ask} { ask() + 5 }
                 command main | (exit: -i32) {
                     let r = handle u() { ask() resume => 1000 + resume(7), return(n) => n };
                     println(r); 0 @ exit
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_handler_may_resume_more_than_once() {
        // Multi-shot: the clause resumes twice and combines both results.
        assert!(
            check(
                "effect C { fn c() -> bool; }
                 fn f() -> i64 / {C} { if c() { 1 } else { 2 } }
                 command main | (exit: -i32) {
                     let r = handle f() {
                         c() resume => resume(true) + resume(false),
                         return(n) => n,
                     };
                     println(r); 0 @ exit
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_trait_method_dispatches_and_bounds_discharge() {
        assert!(
            check(
                "trait Show { fn show(self: +Self) -> String; }
                 impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
                 fn label<T: Show>(x: +T) -> String { show(x) }
                 command main | (exit: -i32) { println(label(1)); 0 @ exit }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_monomorphic_method_call_resolves_for_static_dispatch() {
        // `show(1)` has a concrete receiver, so it resolves to the i64 impl;
        // `show(x)` under `<T: Show>` stays dynamic (the map does not name it).
        let resolve = |s: &str| {
            let toks = lex(s).unwrap();
            let prog = parse(toks).unwrap();
            let (prog, traits) = slc_syntax::traits::elaborate(&prog).expect("elaborate");
            check_program_resolving(&prog, &traits).expect("checks")
        };
        let mono = resolve(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             command main | (exit: -i32) { println(show(1)); 0 @ exit }",
        );
        assert_eq!(mono.methods.len(), 1, "one method call should resolve: {mono:?}");
        assert!(
            matches!(
                mono.methods.values().next(),
                Some(slc_syntax::lower::MethodDispatch::Static(m)) if m.contains("show")
            ),
            "concrete receiver dispatches statically to the impl: {mono:?}"
        );

        let poly = resolve(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             fn label<T: Show>(x: +T) -> String { show(x) }
             command main | (exit: -i32) { println(label(1)); 0 @ exit }",
        );
        // `show(x)` inside `label` projects from the dictionary parameter;
        // `label(1)` passes the concrete i64 dictionary.
        assert!(
            poly.methods.values().any(|d| matches!(
                d,
                slc_syntax::lower::MethodDispatch::Dict { dict_var, .. } if dict_var.contains("Show")
            )),
            "bounded receiver dispatches through a dictionary: {poly:?}"
        );
        assert!(
            poly.calls.values().any(|dicts| dicts.iter().any(|d| d.contains("Show"))),
            "the call to the bounded function passes a dictionary: {poly:?}"
        );
    }

    #[test]
    fn a_negative_function_takes_trait_bounds() {
        // `<T: Show>` works before the params of a `<-` function too.
        assert!(
            check(
                "trait Show { fn show(self: +Self) -> String; }
                 impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
                 fn emit<T: Show>(out: -String, v: +T) <- i64 { show(v) @ out }"
            )
            .is_ok()
        );
        let diags = check(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             fn emit<T>(out: -String, v: +T) <- i64 { show(v) @ out }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("not known to satisfy")), "{diags:?}");
    }

    #[test]
    fn a_method_with_no_impl_is_rejected() {
        let diags = check(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             command main | (exit: -i32) { println(show(true)); 0 @ exit }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no `impl Show for bool`")), "{diags:?}");
    }

    #[test]
    fn an_unbounded_generic_cannot_call_a_method() {
        let diags = check(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             fn bad<T>(x: +T) -> String { show(x) }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("not known to satisfy")), "{diags:?}");
    }

    #[test]
    fn a_let_of_a_value_generalizes() {
        // One binding, three instantiations — through an alias, too: an
        // identifier is a value form.
        assert!(
            check(
                "command main | (exit: -i32) {
                     let f = fn(x) { x };
                     println(f(1) + 1);
                     println(str_len(f(\"s\")));
                     let alias = f;
                     println(alias(true));
                     0 @ exit
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn the_value_restriction_keeps_computations_monomorphic() {
        // The Harper–Lillibridge weapon: a `mu` capture. Generalizing it
        // would let a continuation captured at one instantiation be re-used
        // at another, so it stays monomorphic and mixed uses are rejected.
        let diags = check(
            "command main | (exit: -i32) {
                 let g = mu(k) { fn(x) { x } @ k };
                 println(g(1) + 1);
                 println(str_len(g(\"s\")));
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");

        // An application is not a value either: it may run a command, and
        // its result may hold a captured continuation.
        let diags = check(
            "fn id<T>(x: T) -> T { x }
             command main | (exit: -i32) {
                 let h = id(fn(x) { x });
                 println(h(1) + 1);
                 println(str_len(h(\"s\")));
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");
    }

    #[test]
    fn eta_expansion_recovers_polymorphism_by_name() {
        // Wrapping the capture in a lambda makes it a value: each use
        // re-runs the capture at its own instantiation, visibly.
        assert!(
            check(
                "command main | (exit: -i32) {
                     let fresh = fn(u) { mu(k) { fn(x) { x } @ k } };
                     println(fresh(())(1) + 1);
                     println(str_len(fresh(())(\"s\")));
                     0 @ exit
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_closure_is_checked_at_its_calls() {
        // An unannotated binder is a variable its uses solve together, so
        // two uses cannot disagree.
        let diags = check(
            "command main | (exit: -i32) {
                 let g = fn(x) { x };
                 println(g(1) + str_len(g(1)));
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("expected +String")),
            "closure calls went unchecked once: {diags:?}"
        );

        // The body constrains the parameter, and the call site honors it.
        let diags = check(
            "command main | (exit: -i32) {
                 println(fn(x) { x + 1 }(\"not a number\"));
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the function takes +i64")), "{diags:?}");
    }

    #[test]
    fn a_generic_call_is_instantiated_per_call() {
        // Two calls choose two types.
        assert!(
            check(
                "fn id<T>(x: T) -> T { x }
                 command main | (exit: -i32) {
                     println(id(42) + 1);
                     println(str_len(id(\"each call its own T\")));
                     0 @ exit
                 }"
            )
            .is_ok()
        );

        // Within one call, T is one type.
        let diags = check(
            "fn id<T>(x: T) -> T { x }
             command main | (exit: -i32) { println(str_len(id(42))); 0 @ exit }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");
    }

    #[test]
    fn list_elements_flow_through_the_builtins() {
        // `list_push(list_new(), 42)` makes a `[+i64]`, and its `list_get`
        // continuation must consume an element of it.
        let diags = check(
            "command main | (exit: -i32) {
                 let xs = list_push(list_new(), 42);
                 list_get(
                     xs,
                     0,
                     fn(c: +char) -> ⊥ { println(c); 0 @ exit },
                     fn(m: +String) -> ⊥ { println(m); 1 @ exit },
                 )
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type -char")),
            "the element type should reach the row: {diags:?}"
        );
    }

    #[test]
    fn a_body_produces_what_the_declaration_promises() {
        let diags = check("fn f() -> i64 { \"not an i64\" }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the declaration says +i64")), "{diags:?}");
        // An integer literal still adapts to the declared width.
        assert!(check("fn f() -> i32 { 0 }").is_ok());
        // A body that ends in a cut produces nothing, and promises nothing.
        assert!(check("fn f(k: -i64) <- i64 { 1 @ k }").is_ok());
    }

    #[test]
    fn a_type_parameter_is_rigid_inside_the_body() {
        // `T` is whatever the caller chose, so the body may not treat it as
        // a number…
        let diags = check("fn sneaky<T>(x: T) -> T { x + 1 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("arithmetic operands")), "{diags:?}");

        // …or hand back some other parameter's type.
        let diags = check("fn swap<T, U>(x: T, y: U) -> T { y }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the declaration says")), "{diags:?}");

        assert!(check("fn id<T>(x: T) -> T { x }").is_ok());
    }

    #[test]
    fn shifts_box_and_unbox_consumers() {
        // `↓e` boxes a consumer; boxing data is refused.
        assert!(check("fn f(k: -i64) <- i64 { g(↓k) }").is_ok());
        let diags = check("fn f(x: +i64) -> i64 { g(↓x) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`↓` boxes a consumer")), "{diags:?}");

        // `↑e` opens a box; there has to be one.
        assert!(check("fn f(b: ↓-i64) -> ⊥ { 1 @ ↑b }").is_ok());
        let diags = check("fn f(x: +i64) -> ⊥ { 1 @ ↑x }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`↑` opens a `↓` box")), "{diags:?}");
    }

    #[test]
    fn the_left_of_a_cut_is_data() {
        // Sending a bare consumer would let `¬¬A` pass for `A`.
        let diags = check("fn f(k: -i64, target: ↓↑i64) -> ⊥ { k @ ↑target }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the left of `@` is data")), "{diags:?}");
        assert!(check("fn f(k: -i64, target: ↓↑i64) -> ⊥ { ↓k @ ↑target }").is_ok());
    }

    #[test]
    fn double_negation_does_not_collapse() {
        // `¬¬i64` is `↓↑i64`, and an `i64` is not one: `dne(42)` is the
        // program the shifts exist to reject.
        let diags = check(
            "fn dne(refuter: ↓↑i64) -> i64 { mu(k) { ↓k @ ↑refuter } }
             command main | (exit: -i32) { println(dne(42)); 0 @ exit }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64; the declaration says ↓↑+i64")),
            "{diags:?}"
        );
    }

    #[test]
    fn value_arguments_are_checked_against_the_declaration() {
        let diags = check(
            "fn f(x: +String) -> i64 { 0 }
             command main | (exit: -i32) { println(f(42)); 0 @ exit }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("the declaration says +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn data_is_not_applied() {
        // A `+A` is a value. Applying one used to be accepted, which let a
        // `¬¬A` — the same type, by the involution — be called like a
        // function.
        let diags = check("fn f(x: +i64) -> i64 { x(1) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("which is not a function")), "{diags:?}");
    }

    #[test]
    fn a_select_reads_its_type_off_its_arms() {
        // A bare variant name says which enum, so writing it again is
        // redundant.
        assert!(
            check(
                "enum Color { Red, Green }
                 fn code(return: -i32) <- Color {
                     select { Red <= 0 @ return, Green <= 1 @ return }
                 }"
            )
            .is_ok()
        );

        // Exhaustiveness of an inferred type is checked in `exhaustive`.
    }

    #[test]
    fn a_select_in_a_negative_fn_takes_the_type_it_consumes() {
        // Nothing in `n <= …` names a type, but the declaration already did.
        assert!(check("fn twice(out: -i64) <- +i64 { select { n <= (n * 2) @ out } }").is_ok());
        let diags = check("fn twice(out: -String) <- +i64 { select { n <= str_len(n) @ out } }")
            .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );

        // Outside one, with no arm naming a type, it has to be written.
        let diags = check(
            "command main | (exit: -i32) {
                 let show = select { n <= println(n) };
                 42 @ show;
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no arm names a type")), "{diags:?}");
    }

    #[test]
    fn a_local_mu_takes_its_parameter_type_from_the_body() {
        // `k` is handed to a slot `read_file` declares, so it is `-String`,
        // and the `mu` therefore produces a `+String`.
        let diags = check(
            "command main | (exit: -i32) {
                 let complain = select { m <= { println(m); 1 @ exit } };
                 let text = mu(k) { read_file(\"in\", k, complain) };
                 println(text + 1);
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("+String and +i64")),
            "the inferred type should reach the use: {diags:?}"
        );

        // A cut says it just as well: `42 @ k` makes `k` a consumer of i64.
        let diags = check(
            "command main | (exit: -i32) {
                 let answer = mu(k) { 42 @ k };
                 println(str_len(answer));
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("+i64")), "{diags:?}");
    }

    #[test]
    fn a_select_arm_over_an_atom_binds_the_whole_value() {
        // An atom has one shape and one component, so its arm's pattern is a
        // plain binder, typed by the type being consumed.
        assert!(
            check("fn show(out: -String) <- +i64 { select +i64 { n <= int_to_str(n) @ out } }")
                .is_ok()
        );

        let diags =
            check("fn show(out: -String) <- +i64 { select +i64 { n <= str_len(n) @ out } }")
                .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );
    }
}
