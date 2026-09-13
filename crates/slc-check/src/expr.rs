//! Expression type checking for the ergonomic surface syntax.
//!
//! This pass is intentionally concrete: it validates boolean operators,
//! numeric/comparison operators, indexing, patterns, guards, and annotated
//! bindings using known literal and declaration types. It is not a
//! replacement for the core inference pass; it catches the surface-syntax
//! mistakes that used to appear only at runtime.

use crate::declarations::{Declarations, enum_types};
use crate::env::{Env, constant_types};
use crate::signatures::{FunctionSignature, function_types, instantiate};
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
        Type::Named(n, _) => Some(n.clone()),
        Type::Dual(t) => type_key(t),
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
                "`{callee}` needs `{trait_name}` for a type parameter, but the caller's \
                 type is not known to satisfy it"
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
            && !fits_turning(env, &expected, &actual, arg)
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
    // A method whose `Self` appears only in what it *consumes* — a negative
    // method, `fn deliver(out: -String) <- Self` — learns it from the cut
    // the call stands in, which is checked after this call. Dispatch waits.
    let open = matches!(&target, Type::Var(v)
        if !env.bounds.iter().any(|(bv, bt, _)| bv == v && bt == &trait_name));
    if open {
        env.pending_methods.push(crate::env::PendingMethod {
            span,
            method: method.to_string(),
            trait_name: trait_name.clone(),
            self_ty: self_ty.clone(),
        });
    } else {
        resolve_method_dispatch(method, &trait_name, &target, span, env, diags);
    }
    // A command method returns bottom; a fn method returns its (Self-subst)
    // result type — the consumer of it, for a negative method.
    if sig.is_command {
        return Some(Type::Bottom);
    }
    match &sig.return_type {
        Some(ty) => resolve_with_self(ty, &self_ty, enums).map(|t| {
            let t = match sig.polarity {
                slc_syntax::ast::FunctionPolarity::Negative => t.dual(),
                slc_syntax::ast::FunctionPolarity::Positive => t,
            };
            env.uni.apply(&t)
        }),
        None => Some(Type::One),
    }
}

/// Would these meet, if we tried? The attempt runs on a copy of the
/// unification state, so a stage can ask before committing — a declared
/// callee whose parameters do not take what flows in may still read the
/// other way round, as `⅋` being commutative allows.
fn would_fit(env: &Env, expected: &Type, actual: &Type, expr: Option<&Expr>) -> bool {
    if actual == &Type::Bottom {
        return true;
    }
    let mut probe = env.uni.clone();
    if probe.unify(expected, actual).is_ok() {
        return true;
    }
    expr.is_some_and(is_integer_literal)
        && is_numeric(&env.uni.apply(expected))
        && is_numeric(actual)
}

/// A tuple written in place, weighed component by component against a
/// callee's parameters: an integer literal then takes the width its own
/// slot requires, not the one the whole product happens to have. Like
/// `would_fit`, this commits nothing.
fn fits_piecewise(
    uni: &slc_core::typing::Unification,
    params: &[Type],
    actual: &Type,
    shape: &Expr,
) -> bool {
    let Expr::Pair(items) = shape else { return false };
    let components = tensor_spine(&uni.apply(actual));
    if items.len() != params.len() || items.len() != components.len() {
        return false;
    }
    let mut probe = uni.clone();
    for ((item, actual), param) in items.iter().zip(&components).zip(params) {
        if probe.unify(param, actual).is_ok() {
            continue;
        }
        if commutes(&probe, param, actual) {
            continue;
        }
        if is_integer_literal(&item.kind) && is_numeric(&probe.apply(param)) && is_numeric(actual) {
            continue;
        }
        return false;
    }
    true
}

/// A trait method as a pipeline stage: what flows in is the receiver, so
/// dispatch resolves against its type and the stage's result is the
/// method's. Nothing else about a method changes — it is the same static
/// resolution a call gets, keyed on the stage rather than the call.
fn check_method_stage(
    method: &str,
    receiver: &Type,
    span: Span,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    let sig = env.traits.method_sig(method)?.clone();
    let trait_name = env.traits.method_owner.get(method)?.clone();
    let target = env.uni.apply(receiver);
    resolve_method_dispatch(method, &trait_name, &target, span, env, diags);
    if sig.is_command {
        return Some(Type::Bottom);
    }
    match &sig.return_type {
        Some(ty) => resolve_with_self(ty, &target, enums).map(|t| env.uni.apply(&t)),
        None => Some(Type::One),
    }
}

/// Record how a trait-method call dispatches, once `Self` is known: a
/// concrete receiver calls the impl directly, a bounded type parameter
/// projects the method from the enclosing function's dictionary.
fn resolve_method_dispatch(
    method: &str,
    trait_name: &str,
    target: &Type,
    span: Span,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    discharge_bound(trait_name, target, method, span, env, diags);
    let resolution = match target {
        Type::Var(v) => env.bounds.iter().find(|(bv, bt, _)| bv == v && bt == trait_name).map(
            |(_, _, type_param)| {
                let methods = env.traits.traits.get(trait_name);
                let count = methods.map(|m| m.len()).unwrap_or(1);
                let index =
                    methods.and_then(|m| m.iter().position(|tm| tm.name == method)).unwrap_or(0);
                slc_syntax::lower::MethodDispatch::Dict {
                    dict_var: slc_syntax::lower::dict_param_name(trait_name, type_param),
                    index,
                    count,
                }
            },
        ),
        _ => type_key(target)
            .and_then(|key| env.traits.method_impls.get(method).and_then(|m| m.get(&key)))
            .map(|mangled| slc_syntax::lower::MethodDispatch::Static(mangled.clone())),
    };
    // A static dispatch into a bounded impl — `impl<T: Display> Display for
    // List<T>` — supplies one dictionary per impl bound, read off the
    // receiver's type arguments.
    if let Some(slc_syntax::lower::MethodDispatch::Static(_)) = &resolution
        && let Some(key) = type_key(target)
        && let Some(impl_bounds) =
            env.traits.impl_bounds.get(&(trait_name.to_string(), key)).cloned()
    {
        let receiver_args = scrutinee_args(target).to_vec();
        let mut dict_args = Vec::new();
        for (position, bound_trait) in &impl_bounds {
            let Some(arg) = receiver_args.get(*position) else { continue };
            let arg = env.uni.apply(arg);
            discharge_bound(bound_trait, &arg, method, span, env, diags);
            if let Some(dict) = dict_for(bound_trait, &arg, env, span, diags) {
                dict_args.push(dict);
            }
        }
        if !dict_args.is_empty() {
            env.dispatch.calls.insert(span, dict_args);
        }
    }
    if let Some(resolution) = resolution {
        env.dispatch.methods.insert(span, resolution);
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
        T::Effectful(inner, _) => resolve_with_self(&inner.kind, self_ty, enums),
        _ => enums.resolve(ty),
    }
}

/// The type a `select`'s arms name, when one of them does: a record pattern
/// names its struct, `Color::Red(x)` its enum, and a bare `Red` the enum that
/// declares it.
fn named_by_arms(arms: &[slc_syntax::ast::SelectArm], enums: &Declarations) -> Option<Type> {
    arms.iter().find_map(|arm| match arm.pattern.names()? {
        Named::Declaration(name) if enums.declares(name) => {
            enums.resolve(&TypeExpr::Base(name.to_string()))
        }
        Named::Variant(name) => {
            enums.variant(name).map(|(declaration, _)| Type::Named(declaration.clone(), Vec::new()))
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
        // The signature table speaks only for names nothing local has bound.
        && env.lookup(function).is_none()
        && let Some(slot) =
            args.iter().position(|a| matches!(&a.kind, Expr::Ident(x) if x == name))
        && let Some(ty) = env.functions.get(function).and_then(|s| s.params.get(slot)).cloned()
        // A template variable in a signature is per-call; it says nothing
        // about this parameter.
        && !contains_var(&ty)
    {
        return Some(ty);
    }
    // A cut of two ending in the binder: `v | k⟩` makes `k` the consumer of
    // whatever `v` is. The value is checked again in place, so these
    // diagnostics are thrown away.
    if let Expr::Flow { stages, into_consumer: true, .. } = &body.kind
        && let [value, consumer] = stages.as_slice()
        && matches!(&consumer.kind, Expr::Ident(x) if x == name)
    {
        return check_expr(value, enums, env, &mut Vec::new()).map(|ty| ty.dual());
    }
    body.kind.children().into_iter().find_map(|child| infer_param_type(name, child, enums, env))
}

/// Resolve a declared type, substituting a rigid variable for each type
/// parameter even under a sign: `+T` and `-T` find `T` too.
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
        // The connectives recurse, so a type parameter is found inside a
        // function, tensor, par, or list type too — `(A -> B)` with generic
        // `A` and `B` is a rigid arrow, not an unresolved name.
        T::Fun(a, b) => Some(Type::arrow(
            resolve_rigid(&a.kind, rigid_vars, enums)?,
            resolve_rigid(&b.kind, rigid_vars, enums)?,
        )),
        // The effect row is the effect checker's concern; the type is the
        // arrow underneath.
        T::Effectful(inner, _) => resolve_rigid(&inner.kind, rigid_vars, enums),
        T::Tensor(a, b) => Some(Type::Tensor(
            Box::new(resolve_rigid(&a.kind, rigid_vars, enums)?),
            Box::new(resolve_rigid(&b.kind, rigid_vars, enums)?),
        )),
        T::Par(a, b) => Some(Type::Par(
            Box::new(resolve_rigid(&a.kind, rigid_vars, enums)?),
            Box::new(resolve_rigid(&b.kind, rigid_vars, enums)?),
        )),
        T::With(a, b) => Some(Type::With(
            Box::new(resolve_rigid(&a.kind, rigid_vars, enums)?),
            Box::new(resolve_rigid(&b.kind, rigid_vars, enums)?),
        )),
        T::Apply(name, args) => {
            let args = args
                .iter()
                .map(|a| resolve_rigid(&a.kind, rigid_vars, enums))
                .collect::<Option<Vec<_>>>()?;
            if enums.is_negative_decl(name) {
                Some(Type::Dual(Box::new(Type::Named(name.clone(), args))))
            } else if enums.declares(name) {
                Some(Type::Named(name.clone(), args))
            } else {
                None
            }
        }
        other => enums.resolve(other),
    }
}

/// A declared parameter's type named nothing the checker knows. Leaving the
/// parameter unbound would surface later as "`xs` is not defined", pointing
/// at every use instead of the one cause.
fn unresolved_parameter_type(p: &slc_syntax::ast::Param, span: Span, diags: &mut Vec<Diagnostic>) {
    if let Some(ty) = &p.ty {
        diags.push(Diagnostic {
            message: format!(
                "the type of parameter {} names `{}`, which is not a declared type here; a \
                 library type is `list::List`, or brought in with `use`",
                p.describe(),
                type_display(ty)
            ),
            span,
        });
    }
}

/// The name a written type leads with, for a message.
fn type_display(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Base(name) => name.clone(),
        TypeExpr::Apply(name, _) => format!("{name}<…>"),
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) | TypeExpr::Dual(inner) => {
            type_display(&inner.kind)
        }
        TypeExpr::Effectful(inner, _) => type_display(&inner.kind),
        _ => "this type".into(),
    }
}

/// Resolve a type written in *body* position: a lambda's annotation, a
/// `let`'s, a `select`'s or `mu`'s. The enclosing declaration's type
/// parameters come first, so `T` inside the body is the `T` the signature
/// bound — rigid, and carrying its bounds — rather than a fresh name.
fn resolve_in_body(ty: &TypeExpr, env: &Env, enums: &Declarations) -> Option<Type> {
    if !env.rigid_vars.is_empty() {
        let rigid: HashMap<&str, Type> =
            env.rigid_vars.iter().map(|(name, ty)| (name.as_str(), ty.clone())).collect();
        if let Some(resolved) = resolve_rigid(ty, &rigid, enums) {
            return Some(resolved);
        }
    }
    enums.resolve(ty)
}

/// What a declaration's body-scope replaced, to be restored after it: the
/// bounds in scope, and the type parameters' rigid variables.
type OuterScope = (Vec<(usize, String, String)>, HashMap<String, Type>);

/// Put a declaration's bounds in scope for its body, as (rigid-variable
/// index, trait, type-parameter name), and return the previous set to
/// restore afterward.
fn record_bounds(
    bounds: &[(String, String)],
    rigid_vars: &HashMap<&str, Type>,
    env: &mut Env,
) -> OuterScope {
    let outer = env.bounds.clone();
    for (var, trait_name) in bounds {
        if let Some(Type::Var(v)) = rigid_vars.get(var.as_str()) {
            env.bounds.push((*v, trait_name.clone(), var.clone()));
        }
    }
    // The body resolves written types through these too, so an annotation
    // inside it names the declaration's parameter rather than a fresh one.
    let outer_rigid = std::mem::replace(
        &mut env.rigid_vars,
        rigid_vars.iter().map(|(name, ty)| (name.to_string(), ty.clone())).collect(),
    );
    (outer, outer_rigid)
}

/// Solve the bounded calls a declaration deferred. Everything its body
/// could say about a type parameter has now been said — including what a
/// cut told a call standing in consumer position — so each bound is
/// discharged against what its parameter actually became, and the call's
/// dictionaries recorded for lowering.
fn resolve_pending_dicts(env: &mut Env, diags: &mut Vec<Diagnostic>) {
    for pending in std::mem::take(&mut env.pending_methods) {
        let target = env.uni.apply(&pending.self_ty);
        resolve_method_dispatch(
            &pending.method,
            &pending.trait_name,
            &target,
            pending.span,
            env,
            diags,
        );
    }
    for pending in std::mem::take(&mut env.pending_dicts) {
        let mut dict_args = Vec::new();
        for (trait_name, var) in &pending.bounds {
            let target = env.uni.apply(var);
            discharge_bound(trait_name, &target, &pending.callee, pending.span, env, diags);
            if let Some(dict) = dict_for(trait_name, &target, env, pending.span, diags) {
                dict_args.push(dict);
            }
        }
        env.dispatch.calls.insert(pending.span, dict_args);
    }
}

fn check_decl(d: &Node<Decl>, enums: &Declarations, env: &mut Env, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { name, params, body, polarity, return_type, type_params, bounds, .. } => {
            env.push();
            // A type parameter is rigid inside the body: `T` is some type the
            // caller chose, not a licence to treat the value as any type.
            let rigid_vars: HashMap<&str, Type> =
                type_params.iter().map(|tp| (tp.as_str(), env.uni.fresh_rigid())).collect();
            let (outer_bounds, outer_rigid) = record_bounds(bounds, &rigid_vars, env);
            let rigid = |ty: &TypeExpr| resolve_rigid(ty, &rigid_vars, enums);
            for p in params {
                match p.ty.as_ref().and_then(&rigid) {
                    Some(ty) => bind_match_pattern(&p.pattern, &ty, enums, env),
                    None => unresolved_parameter_type(p, d.span, diags),
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
            resolve_pending_dicts(env, diags);
            env.bounds = outer_bounds;
            env.rigid_vars = outer_rigid;
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
                && !fits_turning(env, promised, actual, tail_node(body))
            {
                diags.push(Diagnostic {
                    message: format!(
                        "the body of `{name}` has type {actual}; the declaration says {promised}"
                    ),
                    span: body.span,
                });
            }
        }
        Decl::Command {
            value_params,
            continuation_params,
            body,
            return_type,
            type_params,
            bounds,
            ..
        } => {
            if let Some(return_type) = return_type
                && enums.resolve(return_type) != Some(Type::Bottom)
            {
                diags.push(Diagnostic {
                    message: "a `command` returns `Bottom` (`⊥`); this `Bottom` is not the nullary prelude form"
                        .into(),
                    span: d.span,
                });
            }
            env.push();
            let rigid_vars: HashMap<&str, Type> =
                type_params.iter().map(|tp| (tp.as_str(), env.uni.fresh_rigid())).collect();
            let (outer_bounds, outer_rigid) = record_bounds(bounds, &rigid_vars, env);
            for p in value_params.iter().chain(continuation_params.iter()) {
                match p.ty.as_ref().and_then(|ty| resolve_rigid(ty, &rigid_vars, enums)) {
                    Some(ty) => bind_match_pattern(&p.pattern, &ty, enums, env),
                    None => unresolved_parameter_type(p, d.span, diags),
                }
            }
            let body_type = check_expr(body, enums, env, diags);
            // A `command` consumes: every terminating path must reach a
            // continuation, so the body is `⊥`. A body that produces a value
            // (a bare value, or an `if` that falls through with no `else`)
            // does not, and is rejected. Which continuation, or how many, is
            // not constrained — the core is classical. A body whose type the
            // checker cannot pin down is left alone.
            if let Some(actual) = body_type {
                let actual = env.uni.apply(&actual);
                if actual != Type::Bottom && !matches!(actual, Type::Var(_)) {
                    diags.push(Diagnostic {
                        message: format!(
                            "a `command` body must reach a continuation on every path (type `⊥`); \
                             this one has type {actual}"
                        ),
                        span: body.span,
                    });
                }
            }
            resolve_pending_dicts(env, diags);
            env.bounds = outer_bounds;
            env.rigid_vars = outer_rigid;
            env.pop();
        }
        Decl::Const { name, ty, value, .. } => {
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
        Expr::Pair(items) | Expr::Bundle(items) => {
            items.iter().all(|item| is_value_form(&item.kind, enums))
        }
        Expr::Data { fields, .. } => {
            fields.iter().all(|(_, value)| is_value_form(&value.kind, enums))
        }
        // A constructor applied to values builds data; any other call runs.
        Expr::Call { callee, args } => {
            matches!(&callee.kind, Expr::Ident(name) if enums.variant(name).is_some())
                && args.iter().all(|arg| is_value_form(&arg.kind, enums))
        }
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

/// The type arguments a scrutinee carries, seen through `Dual` — what a
/// generic declaration's stored types are instantiated with at this use.
fn scrutinee_args(scrutinee: &Type) -> &[Type] {
    match scrutinee {
        Type::Named(_, args) => args,
        Type::Dual(inner) => scrutinee_args(inner),
        _ => &[],
    }
}

/// The dictionary witnessing `ty: bound_trait` — the enclosing function's
/// own parameter for a bound rigid variable, a global for a concrete type,
/// and for a bounded impl the global *constructed*: applied to one
/// dictionary per impl bound, read off the type's arguments, recursively.
fn dict_for(
    bound_trait: &str,
    ty: &Type,
    env: &mut Env,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) -> Option<slc_syntax::lower::DictExpr> {
    if let Type::Var(v) = ty {
        return env.bounds.iter().find(|(bv, bt, _)| bv == v && bt == bound_trait).map(
            |(_, _, tp)| slc_syntax::lower::DictExpr {
                name: slc_syntax::lower::dict_param_name(bound_trait, tp),
                args: Vec::new(),
            },
        );
    }
    let key = type_key(ty)?;
    if !env.traits.has_impl(bound_trait, &key) {
        return None;
    }
    let mut args = Vec::new();
    if let Some(impl_bounds) =
        env.traits.impl_bounds.get(&(bound_trait.to_string(), key.clone())).cloned()
    {
        // Construction applies the impl's methods to the inner
        // dictionaries; a multi-method dictionary is a tuple, which an
        // application cannot thread through.
        let methods = env.traits.traits.get(bound_trait).map(|m| m.len()).unwrap_or(1);
        if methods > 1 {
            diags.push(Diagnostic {
                message: format!(
                    "`{bound_trait}` has several methods, and its impl for `{key}` is \
                     bounded; constructing that dictionary is not supported yet"
                ),
                span,
            });
            return None;
        }
        let ty_args = scrutinee_args(ty).to_vec();
        for (position, inner_trait) in impl_bounds {
            let inner_ty = env.uni.apply(ty_args.get(position)?);
            args.push(dict_for(&inner_trait, &inner_ty, env, span, diags)?);
        }
    }
    Some(slc_syntax::lower::DictExpr {
        name: slc_syntax::lower::dict_global_name(bound_trait, &key),
        args,
    })
}

/// Fresh unification variables for a declaration's type parameters, ready
/// to instantiate its stored field and payload types at one use.
fn fresh_args(
    enums: &Declarations,
    name: &str,
    uni: &mut slc_core::typing::Unification,
) -> Vec<Type> {
    (0..enums.arity(name)).map(|_| uni.fresh_var()).collect()
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
        // A bare name that several enums claim resolves to nothing, and a
        // pattern binder that silently catches everything is worse than an
        // error: qualify it, or import one enum's variants.
        Pattern::Ident(name) if declarations.is_ambiguous_variant(name) => {
            diags.push(Diagnostic {
                message: format!(
                    "`{name}` is a variant of more than one enum; qualify it, or pin one \
                     with `use Enum::{{{name}}};`"
                ),
                span,
            });
        }
        Pattern::Enum { name, variant, .. }
            if variant.is_empty() && declarations.is_ambiguous_variant(name) =>
        {
            diags.push(Diagnostic {
                message: format!(
                    "`{name}` is a variant of more than one enum; qualify it, or pin one \
                     with `use Enum::{{{name}}};`"
                ),
                span,
            });
        }
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
        Pattern::Tuple(items) | Pattern::Bundle(items) => {
            for item in items {
                check_pattern(item, expected, declarations, span, diags);
            }
        }
        // A request shape matches a continuation of its menu type — the
        // positive `Named` that is the menu's dual.
        Pattern::Dtor { dtor, .. } => match declarations.destructor(dtor) {
            Some((menu, _)) => {
                if expected != &Type::Named(menu.clone(), Vec::new()) {
                    diags.push(Diagnostic {
                        message: format!(
                            "request pattern `.{dtor}` matches a continuation of `{menu}`; \
                                 scrutinee has type {expected}"
                        ),
                        span,
                    });
                }
            }
            None => diags.push(Diagnostic {
                message: format!("`.{dtor}` does not name a declared menu item"),
                span,
            }),
        },
        // A record pattern decomposes the product: the same fields, in the
        // same order, with the same types as the declaration.
        Pattern::Data { name, fields } => {
            let Some(declared) = declarations.records.get(name) else {
                diags.push(Diagnostic {
                    message: format!("`{name}` is not a declared record"),
                    span,
                });
                return;
            };
            if expected != &Type::Named(name.clone(), Vec::new())
                && !(declarations.is_unit_record(name) && expected == &Type::One)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "record pattern `{name}` cannot match a scrutinee of type {expected}"
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
        _ => {}
    }
}

fn expected_index_type(value_ty: &Type) -> Option<Type> {
    match value_ty {
        Type::Pos(Base::Str) => Some(Type::Pos(Base::I64)),
        _ => None,
    }
}

fn index_result_type(value_ty: &Type) -> Option<Type> {
    match value_ty {
        Type::Pos(Base::Str) => Some(Type::Pos(Base::Char)),
        _ => None,
    }
}

/// Does a value written as `expr`, inferred as `actual`, fit a port that
/// requires `expected`?
///
/// An integer literal takes the integer type its port requires — `0 | exit`
/// sends an `i32` — and is `+i64` only when nothing constrains it. Every
/// other value must match its port exactly.
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

/// `A ⅋ B` and `B ⅋ A` are one type, so a value of one may be cut into a
/// consumer of the other. The unifier stays structural — a value of `⅋` is a
/// closure facing one way, and a commutation buried inside a constructor has
/// no single value to turn around — so this is tried only where one value
/// meets one consumer, after the forward reading has failed, and what it
/// returns tells lowering how to turn the value. Both halves must be known:
/// the swap is oriented by their polarities.
fn commute(env: &mut Env, expected: &Type, actual: &Type) -> Option<slc_syntax::lower::Swap> {
    let (Type::Par(want_left, want_right), Type::Par(left, right)) =
        (env.uni.apply(expected), env.uni.apply(actual))
    else {
        return None;
    };
    let mut probe = env.uni.clone();
    if probe.unify(&want_left, &right).is_err() || probe.unify(&want_right, &left).is_err() {
        return None;
    }
    let (left, right) = (probe.apply(&left), probe.apply(&right));
    if contains_var(&left) || contains_var(&right) {
        return None;
    }
    env.uni = probe;
    Some(slc_syntax::lower::Swap {
        left_positive: left.is_positive(),
        right_positive: right.is_positive(),
    })
}

/// Forward, or else the mirrored `⅋` reading, recorded as a swap on `value`
/// so lowering turns it around. If neither fits, the unifier is left as the
/// forward attempt left it, so a refusal reads exactly as it did before.
fn fits_turning(env: &mut Env, expected: &Type, actual: &Type, value: &Node<Expr>) -> bool {
    let before = env.uni.clone();
    if fits(env, expected, actual, &value.kind) {
        return true;
    }
    let failed = std::mem::replace(&mut env.uni, before);
    match commute(env, expected, actual) {
        Some(swap) => {
            env.dispatch.swaps.insert(value.span, swap);
            true
        }
        None => {
            env.uni = failed;
            false
        }
    }
}

/// Whether `commute` would succeed, without committing — for the probes that
/// decide which arm a stage takes.
fn commutes(uni: &slc_core::typing::Unification, expected: &Type, actual: &Type) -> bool {
    let (Type::Par(want_left, want_right), Type::Par(left, right)) =
        (uni.apply(expected), uni.apply(actual))
    else {
        return false;
    };
    let mut probe = uni.clone();
    probe.unify(&want_left, &right).is_ok()
        && probe.unify(&want_right, &left).is_ok()
        && !contains_var(&probe.apply(&left))
        && !contains_var(&probe.apply(&right))
}

/// `tail_expr`, as the node, so a swap on a body's value can be keyed by the
/// span of the expression that produces it.
fn tail_node(e: &Node<Expr>) -> &Node<Expr> {
    match &e.kind {
        Expr::Block(items) => items.last().map(tail_node).unwrap_or(e),
        Expr::Let { body: Some(body), .. } => tail_node(body),
        _ => e,
    }
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
/// type, and no position may be added, dropped, or reordered. A row is a
/// fixed calling interface, so one that differs in width or order is a
/// different interface, not a compatible one. Value arguments are checked
/// against their declared types the same way — builtins excepted, whose
/// arguments the builtin table already checks.
fn check_call_arguments(
    name: &str,
    signature: &FunctionSignature,
    args: &[Node<Expr>],
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let row_width = signature.continuations.iter().filter(|is_cont| **is_cont).count();
    let values = signature.continuations.iter().filter(|is_cont| !**is_cont).count();
    // A row of several exits may arrive spread, one argument per exit, or
    // whole — one bundle, which is that menu. Both are the same call: the
    // spread form packs, and the bundle already is the packed form.
    if row_width > 1 && args.len() == values + 1 {
        for (index, arg) in args.iter().enumerate() {
            let actual = check_expr(arg, enums, env, diags);
            if index < values {
                if signature.builtin {
                    continue;
                }
                if let (Some(expected), Some(actual)) = (signature.params.get(index), &actual)
                    && !fits(env, expected, actual, &arg.kind)
                {
                    let expected = &env.uni.apply(expected);
                    diags.push(Diagnostic {
                        message: format!(
                            "argument {} of `{name}` has type {actual}; expected {expected}",
                            index + 1
                        ),
                        span: arg.span,
                    });
                }
                continue;
            }
            let row = signature.params[values..]
                .iter()
                .rev()
                .cloned()
                .reduce(|acc, ty| Type::With(Box::new(ty), Box::new(acc)));
            if let (Some(row), Some(actual)) = (row, &actual)
                && !fits(env, &row, actual, &arg.kind)
            {
                let row = env.uni.apply(&row);
                diags.push(Diagnostic {
                    message: format!(
                        "continuation row mismatch: `{name}` offers the exits {row}, and \
                         this bundle has type {actual}"
                    ),
                    span: arg.span,
                });
            }
        }
        return;
    }
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
        if !in_row && signature.builtin {
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
/// Bind what a `let` pattern names. A bare name is the trivial pattern, and
/// the only one that generalizes: a value form bound to a name may be used
/// at several types, while a destructured part is a component of one value
/// the pattern has already fixed.
fn bind_let_pattern(
    pattern: &slc_syntax::ast::Pattern,
    binding_ty: Type,
    value: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
) {
    if let Some(name) = pattern.binder_name() {
        let (binding_ty, generalized) = generalize(binding_ty, &value.kind, enums, env);
        env.define_scheme(name, binding_ty, generalized);
        return;
    }
    bind_match_pattern(pattern, &binding_ty, enums, env);
}

fn check_let_binding(
    pattern: &slc_syntax::ast::Pattern,
    ty: &Option<TypeExpr>,
    value: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Type {
    let actual = check_expr(value, enums, env, diags);
    let annotation = ty.as_ref().and_then(|ty| resolve_in_body(ty, env, enums));
    if let (Some(annotation), Some(actual)) = (&annotation, actual.clone())
        && !fits_turning(env, annotation, &actual, value)
    {
        diags.push(Diagnostic {
            message: match pattern.binder_name() {
                Some(name) => format!(
                    "`let {name}` is annotated as {annotation}; initializer has type {actual}"
                ),
                None => format!(
                    "this `let` is annotated as {annotation}; initializer has type {actual}"
                ),
            },
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
            // A generic scrutinee's arguments instantiate the payload types.
            let args = scrutinee_args(scrutinee);
            for (field, ty) in fields.iter().zip(payload.iter()) {
                bind_match_pattern(field, &ty.instantiate(args), enums, env);
            }
        }
        Pattern::Data { name, fields } => {
            let declared = enums.fields(name).unwrap_or_default();
            let args = scrutinee_args(scrutinee);
            for ((_, field), ty) in fields.iter().zip(declared.iter()) {
                bind_match_pattern(field, &ty.instantiate(args), enums, env);
            }
        }
        // A request shape binds the continuation it carries. In a `match`
        // the payload is a live continuation — opaque at run time — so only
        // a binder can take it; nesting belongs to `mu`, where dispatch is
        // deferred.
        Pattern::Dtor { dtor, arg } => {
            if let Some(ty) = enums.destructor(dtor).and_then(|(_, p)| p.first()) {
                bind_match_pattern(arg, &ty.instantiate(scrutinee_args(scrutinee)), enums, env);
            }
        }
        Pattern::Tuple(items) => {
            let components = flatten_tensor(scrutinee);
            for (item, ty) in items.iter().zip(components.iter()) {
                bind_match_pattern(item, ty, enums, env);
            }
        }
        // A bundle binds the exits of an anonymous menu, as a tuple binds
        // the components of a product.
        Pattern::Bundle(items) => {
            let components = flatten_with(scrutinee);
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

/// Check the arms of a copattern `mu` against its menu, recursively: each
/// arm binds its request's continuation (or refines it with nested
/// copatterns into an inner menu) and answers with a command.
fn check_comatch_arms(
    menu: &str,
    type_args: &[Type],
    rows: Vec<(&slc_syntax::ast::Pattern, &Node<Expr>)>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    use slc_syntax::ast::Pattern;
    let mut order: Vec<&String> = Vec::new();
    let mut groups: std::collections::HashMap<&String, Vec<(&Pattern, &Node<Expr>)>> =
        std::collections::HashMap::new();
    let check_command = |env: &mut Env, command: &Node<Expr>, diags: &mut Vec<Diagnostic>| {
        let ty = check_expr(command, enums, env, diags);
        if let Some(ty) = ty
            && ty != Type::Bottom
            && ty != Type::One
        {
            diags.push(Diagnostic {
                message: format!("a `mu` arm is a command; this one has type {ty}"),
                span: command.span,
            });
        }
    };
    for (pattern, command) in rows {
        let Pattern::Dtor { dtor, arg } = pattern else {
            diags.push(Diagnostic {
                message: format!("`mu {menu}` answers demands; every arm is `.item(p)`"),
                span: command.span,
            });
            continue;
        };
        if !groups.contains_key(dtor) {
            order.push(dtor);
        }
        groups.entry(dtor).or_default().push((arg.as_ref(), command));
    }
    for dtor in order {
        let group = groups.remove(dtor).expect("grouped above");
        let Some((_, payload)) = enums.destructor(&format!("{menu}::{dtor}")) else {
            diags.push(Diagnostic {
                message: format!("`{menu}` has no item `{dtor}`"),
                span: group[0].1.span,
            });
            for (_, command) in group {
                check_command(env, command, diags);
            }
            continue;
        };
        let k_ty = payload.first().cloned().unwrap_or(Type::One).instantiate(type_args);
        let mut nested: Vec<(&Pattern, &Node<Expr>)> = Vec::new();
        for (arg, command) in group {
            match arg {
                Pattern::Ident(name) => {
                    env.push();
                    env.define(name, k_ty.clone());
                    check_command(env, command, diags);
                    env.pop();
                }
                Pattern::Wildcard => check_command(env, command, diags),
                Pattern::Dtor { .. } => nested.push((arg, command)),
                _ => {
                    diags.push(Diagnostic {
                        message: format!(
                            "`.{dtor}` carries a continuation: bind it, or refine it with a \
                             nested request"
                        ),
                        span: command.span,
                    });
                    check_command(env, command, diags);
                }
            }
        }
        if !nested.is_empty() {
            // A refined item's answer must itself be a menu.
            match enums.nested_menu(&format!("{menu}::{dtor}")) {
                Some(inner) => {
                    let inner = inner.to_string();
                    // The refined item's answer carries the inner menu's
                    // arguments: `k_ty` is `Named(inner, args)` after
                    // instantiation, seen through nothing — a request type.
                    let inner_args = scrutinee_args(&k_ty).to_vec();
                    check_comatch_arms(&inner, &inner_args, nested, enums, env, diags);
                }
                None => {
                    diags.push(Diagnostic {
                        message: format!(
                            "`.{dtor}` answers {}, which is not a menu, so its request cannot \
                             be refined",
                            k_ty.dual()
                        ),
                        span: nested[0].1.span,
                    });
                    for (_, command) in nested {
                        check_command(env, command, diags);
                    }
                }
            }
        }
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
        if declarations.is_ambiguous_variant(name) {
            diags.push(Diagnostic {
                message: format!(
                    "`{name}` is a variant of more than one enum; qualify it, or pin one \
                     with `use Enum::{{{name}}};`"
                ),
                span,
            });
            return;
        }
        env.define(name, consumed.clone());
        return;
    }
    if matches!(pattern, Pattern::Wildcard) {
        return;
    }
    let components: Vec<Type> = match (consumed, pattern) {
        // An enum variant: its payload types.
        (Type::Named(_, _), Pattern::Ident(_) | Pattern::Enum { .. }) => {
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
                Some((_, payload)) => {
                    payload.iter().map(|t| t.instantiate(scrutinee_args(consumed))).collect()
                }
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
        (Type::One, Pattern::Data { name, .. }) if declarations.is_unit_record(name) => Vec::new(),
        (Type::One, Pattern::Tuple(items)) if items.is_empty() => Vec::new(),
        (Type::Named(name, _), Pattern::Data { name: written, .. }) => {
            if written != name {
                diags.push(Diagnostic {
                    message: format!("`select {consumed}` arm cannot bind a `{written}`"),
                    span,
                });
                return;
            }
            declarations
                .fields(name)
                .unwrap_or_default()
                .iter()
                .map(|t| t.instantiate(scrutinee_args(consumed)))
                .collect()
        }
        // A tensor: its components, flattened right-nested.
        (Type::Tensor(..), Pattern::Tuple(_)) => flatten_tensor(consumed),
        // A menu of exits: its items, likewise.
        (Type::With(..), Pattern::Bundle(_)) => flatten_with(consumed),
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
        Pattern::Data { fields, .. } => fields.iter().map(|(_, p)| p).collect(),
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
        bind_select_component(binder, &ty, declarations, env, span, diags);
    }
}

/// Bind one component of a `select` arm. A nested product — a tuple or a
/// record — has one shape, so it may be taken apart in place; a sum inside a
/// component needs its own `match` in the arm.
fn bind_select_component(
    pattern: &slc_syntax::ast::Pattern,
    ty: &Type,
    declarations: &Declarations,
    env: &mut Env,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use slc_syntax::ast::Pattern;
    match pattern {
        Pattern::Ident(name) => env.define(name, ty.clone()),
        Pattern::Wildcard => {}
        Pattern::Tuple(items) => {
            let components = flatten_tensor(ty);
            if items.len() != components.len() {
                diags.push(Diagnostic {
                    message: format!(
                        "this component has {} part(s); the pattern binds {}",
                        components.len(),
                        items.len()
                    ),
                    span,
                });
            }
            for (item, ty) in items.iter().zip(components) {
                bind_select_component(item, &ty, declarations, env, span, diags);
            }
        }
        Pattern::Data { name, fields } => {
            let declared = declarations.fields(name).unwrap_or_default();
            for ((_, field), ty) in fields.iter().zip(declared.iter()) {
                bind_select_component(field, ty, declarations, env, span, diags);
            }
        }
        _ => diags.push(Diagnostic {
            message: "a `select` arm covers one shape: a sum or a value inside a component \
                      needs its own `match` in the arm"
                .into(),
            span,
        }),
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

/// The items of a `&`, flattened right-nested — the exits a bundle holds.
fn flatten_with(ty: &Type) -> Vec<Type> {
    match ty {
        Type::With(a, b) => {
            let mut items = vec![(**a).clone()];
            items.extend(flatten_with(b));
            items
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

/// The components of a right-nested product, flattened along the spine, so
/// `A ⊗ (B ⊗ C)` is `[A, B, C]` — matching the runtime projection walk. A
/// non-product is a single component.
/// The components a positional projection can reach: a product's, and a
/// menu-of-exits' — taking an exit is projecting an item of a `&`, which
/// the runtime holds as the same right-nested pair.
fn tensor_spine(ty: &Type) -> Vec<Type> {
    let mut out = Vec::new();
    let mut current = ty.clone();
    loop {
        match current {
            Type::Tensor(a, b) | Type::With(a, b) => {
                out.push(*a);
                current = *b;
            }
            other => {
                out.push(other);
                return out;
            }
        }
    }
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
            // A function referenced as a value has its signature's type,
            // curried — which is what lets an argument like `map(double, …)`
            // solve the type parameters `double` pins down. A *bounded*
            // function as a value would need its dictionaries packaged with
            // it, which nothing builds yet, so it stays untyped here.
            if let Some(signature) = env.functions.get(name)
                && signature.bounds.is_empty()
            {
                let (signature, _) = instantiate(signature, &mut env.uni);
                if let Some(result) = signature.result {
                    // Its parameters pack into the one product a call
                    // passes; a function with none is its result already,
                    // since naming it is how it is used.
                    let ty = match signature
                        .params
                        .into_iter()
                        .rev()
                        .reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
                    {
                        Some(packed) => Type::arrow(packed, result),
                        None => result,
                    };
                    return Some(ty);
                }
                return None;
            }
            let Some((declaration, payload)) = enums.variant(name) else {
                // A trait method named as a value is dispatched where it is
                // applied, and a bounded function as a value would need its
                // dictionaries packaged with it: both stay untyped here.
                // Anything else is unknown, and the runtime is the wrong
                // place to find that out.
                if !env.traits.is_method(name) && !env.functions.contains_key(name) {
                    diags.push(Diagnostic {
                        message: match name.split_once("::") {
                            Some((module, _)) => {
                                format!("`{name}` is not defined; is `{module}` a module in scope?")
                            }
                            None => format!(
                                "`{name}` is not defined here; a library name is reached by \
                                 its module's path, or brought in with `use`"
                            ),
                        },
                        span: e.span,
                    });
                }
                return None;
            };
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
            // A payloadless variant of a generic declaration — `List::Nil`
            // — is a value at any instantiation.
            let declaration = declaration.clone();
            let type_args = fresh_args(enums, &declaration, &mut env.uni);
            Some(Type::Named(declaration, type_args))
        }
        Expr::Lambda { param, param_type, body, .. } => {
            env.push();
            let param_ty = param_type
                .as_ref()
                .and_then(|ty| resolve_in_body(ty, env, enums))
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
                // A generic declaration's payload types carry its
                // parameters; each construction instantiates them fresh.
                let type_args = fresh_args(enums, &declaration, &mut env.uni);
                let payload: Vec<Type> =
                    payload.iter().map(|t| t.instantiate(&type_args)).collect();
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
                        && !fits_turning(env, expected, &actual, arg)
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
                return Some(Type::Named(declaration, type_args));
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
                         with a cut: `value | {name}⟩`"
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
            // A parameter shadows a global of the same name: the signature
            // table speaks only for names nothing local has bound.
            if let Expr::Ident(name) = &callee.kind
                && env.lookup(name).is_none()
                && let Some(signature) = env.functions.get(name)
            {
                let (signature, seen) = instantiate(signature, &mut env.uni);
                if signature.builtin {
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
                // A function is applied by flowing into it: `x | f`, not
                // `f(x)`. A command takes both its groups from the chain —
                // `(xs, i) | nth | (found & missing)⟩` — and a constructor
                // builds rather than applies, so only those keep parens.
                if !args.is_empty() && !signature.builtin {
                    let piped = if args.len() == 1 {
                        format!("{} | {name}", "argument")
                    } else {
                        format!("(…, …) | {name}")
                    };
                    diags.push(Diagnostic {
                        message: format!(
                            "`{name}` is a function, and a function is applied by flowing \
                             into it: write `{piped}`"
                        ),
                        span: e.span,
                    });
                }
                check_call_arguments(name, &signature, args, enums, env, diags);
                // Say where the value product ends and the menu of exits
                // begins, so lowering packs each group into one argument.
                let values = signature.continuations.iter().filter(|c| !**c).count();
                if signature.continuations.iter().any(|c| *c) {
                    env.dispatch.call_groups.insert(e.span, values);
                }
                // Discharge each bound against what its type parameter
                // resolved to, now that the arguments have constrained it, and
                // record the dictionary the call must pass for it: the global
                // dict of a concrete type, or the enclosing function's own
                // dict parameter when the bound is forwarded.
                if !signature.bounds.is_empty() {
                    let bounds = signature
                        .bounds
                        .iter()
                        .filter_map(|(param_index, trait_name)| {
                            seen.get(param_index).map(|var| (trait_name.clone(), var.clone()))
                        })
                        .collect();
                    env.pending_dicts.push(crate::env::PendingDicts {
                        span: e.span,
                        callee: name.clone(),
                        bounds,
                    });
                }
                return signature.result.map(|ty| env.uni.apply(&ty));
            }
            // A local callee: a closure, or a binder whose type its uses
            // decide. `A → B` is `-A ⅋ B`, so application peels a `⅋`, and
            // an unknown callee becomes one.
            let callee_ty = callee_ty.map(|ty| env.uni.apply(&ty));
            match callee_ty {
                Some(Type::Par(argument_dual, result)) => {
                    // The arguments pack into one product, as they do for a
                    // named callee, so the whole group meets the one type
                    // the function takes.
                    let actuals: Vec<Option<Type>> =
                        args.iter().map(|arg| check_expr(arg, enums, env, diags)).collect();
                    let packed = actuals
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .and_then(|types| {
                            types
                                .into_iter()
                                .rev()
                                .reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
                        })
                        .unwrap_or(Type::One);
                    let expected = argument_dual.dual();
                    let shape = args.first().map(|a| a.kind.clone());
                    if !fits(env, &expected, &packed, shape.as_ref().unwrap_or(&e.kind)) {
                        let expected = env.uni.apply(&expected);
                        diags.push(Diagnostic {
                            message: format!(
                                "this call's arguments have type {packed}; the function takes \
                                 {expected}"
                            ),
                            span: args.first().map(|a| a.span).unwrap_or(e.span),
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
                // No `else`: the then-branch's value is discarded and the
                // false path yields unit, so the `if` is a unit statement —
                // never `⊥`, even when the then-branch ends in a cut.
                return Some(Type::One);
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
        Expr::Let { pattern, ty, value, body } => {
            let binding_ty = check_let_binding(pattern, ty, value, enums, env, diags);
            env.push();
            bind_let_pattern(pattern, binding_ty, value, enums, env);
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
            value_ty
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
        Expr::Data { name, fields } => {
            let type_args = fresh_args(enums, name, &mut env.uni);
            let Some(declared) = enums.records.get(name).cloned() else {
                diags.push(Diagnostic {
                    message: format!("`{name}` is not a declared record"),
                    span: e.span,
                });
                for (_, value) in fields {
                    check_expr(value, enums, env, diags);
                }
                return None;
            };
            let declared: Vec<(String, Type)> =
                declared.into_iter().map(|(f, t)| (f, t.instantiate(&type_args))).collect();
            // A record literal is the product of its declared fields: every
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
                    && !fits_turning(env, expected, &actual, value)
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
            if enums.is_unit_record(name) {
                Some(Type::One)
            } else {
                Some(Type::Named(name.clone(), type_args))
            }
        }
        Expr::CoMatch { ty, arms } => {
            // `mu T { item: k <= c, … }` — the copattern form of `mu`: a
            // menu value, branching on the demand the ambient consumer turns
            // out to be. Each arm binds the continuation its request carries
            // and answers it with a command.
            // The menu's name — written bare or applied to type arguments —
            // or read off an arm's destructor.
            let named = match ty.as_deref().map(|ty| &ty.kind) {
                Some(TypeExpr::Base(n)) if enums.is_menu(n) => Some((n.clone(), None)),
                Some(TypeExpr::Apply(n, args)) if enums.is_menu(n) => {
                    let args = args
                        .iter()
                        .map(|a| resolve_in_body(&a.kind, env, enums))
                        .collect::<Option<Vec<_>>>();
                    Some((n.clone(), args))
                }
                Some(_) => {
                    diags.push(Diagnostic {
                        message: "`mu` with arms builds a menu; write a declared menu name".into(),
                        span: ty.as_ref().map(|t| t.span).unwrap_or(e.span),
                    });
                    None
                }
                // Left out: an arm's destructor may name it.
                None => arms
                    .iter()
                    .find_map(|arm| match &arm.pattern {
                        slc_syntax::ast::Pattern::Dtor { dtor, .. } => {
                            enums.destructor(dtor).map(|(menu, _)| menu.clone())
                        }
                        _ => None,
                    })
                    .map(|menu| (menu, None)),
            };
            let Some((menu, written_args)) = named else {
                diags.push(Diagnostic {
                    message: "no arm names a menu item, so write the menu: `mu Config { … }`"
                        .into(),
                    span: e.span,
                });
                return None;
            };
            // A generic menu instantiates fresh at each construction; the
            // arms' answers constrain the arguments.
            let type_args = written_args.unwrap_or_else(|| fresh_args(enums, &menu, &mut env.uni));
            let rows: Vec<(&slc_syntax::ast::Pattern, &Node<Expr>)> =
                arms.iter().map(|arm| (&arm.pattern, &arm.command)).collect();
            check_comatch_arms(&menu, &type_args, rows, enums, env, diags);
            Some(Type::Dual(Box::new(Type::Named(menu, type_args))))
        }
        Expr::Select { ty, arms } => {
            // `select T { p => c, … }` builds the consumer of T. Each arm
            // covers one shape of T, binds that shape's components, and runs
            // a command; the whole expression is dual to T.
            let resolved = match ty {
                Some(ty) => match resolve_in_body(&ty.kind, env, enums) {
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
            let selects_bottom_alias = ty
                .as_deref()
                .and_then(|ty| match &ty.kind {
                    TypeExpr::Base(name) => Some(name.as_str()),
                    _ => None,
                })
                .is_some_and(|name| enums.is_bottom_alias(name))
                || arms.iter().any(|arm| {
                    matches!(&arm.pattern,
                        slc_syntax::ast::Pattern::Data { name, .. }
                            if enums.is_bottom_alias(name))
                });
            // A menu belongs to `mu`: `select` answers data, and a menu
            // answers demands.
            if let Type::Dual(inner) = &resolved
                && let Type::Named(menu, _) = inner.as_ref()
                && enums.is_menu(menu)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`select` answers data, and `{menu}` is a menu, which answers demands: \
                         build it with `mu {menu} {{ … }}`"
                    ),
                    span: e.span,
                });
                return None;
            }
            // `select F { F { a, b } => c }` over a form builds the form
            // value itself: it consumes the record its fields describe, so
            // the arm binds that record's components.
            if let Type::Dual(inner) = &resolved
                && let Type::Named(form, _) = inner.as_ref()
                && enums.is_form(form)
            {
                let demand = Type::Named(form.clone(), Vec::new());
                for arm in arms {
                    env.push();
                    bind_select_arm(&demand, &arm.pattern, enums, env, e.span, diags);
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
                return Some(resolved);
            }
            // An unsolved variable is not yet anything — a generic `<- T`
            // body selects over the rigid `T` its caller chose.
            if resolved.is_negative() && !matches!(resolved, Type::Var(_)) && !selects_bottom_alias
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`select` consumes data, and {resolved} is a consumer; `select` \
                         builds the consumer of a positive type"
                    ),
                    span: e.span,
                });
                return None;
            }
            let consumed = if selects_bottom_alias { Type::One } else { resolved.clone() };
            for arm in arms {
                env.push();
                bind_select_arm(&consumed, &arm.pattern, enums, env, e.span, diags);
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
            if selects_bottom_alias { Some(Type::Bottom) } else { Some(resolved.dual()) }
        }
        // `.item(k)` — a request: the continuation `k` must consume the
        // item's answer, and the request itself is the dual of the menu.
        Expr::Request { dtor, arg } => {
            let Some((menu, payload)) = enums.destructor(dtor).cloned() else {
                diags.push(Diagnostic {
                    message: format!("`.{dtor}` does not name a declared menu item"),
                    span: e.span,
                });
                check_expr(arg, enums, env, diags);
                return None;
            };
            if !enums.is_menu(&menu) {
                diags.push(Diagnostic {
                    message: format!("`.{dtor}` names an enum variant, not a menu item"),
                    span: e.span,
                });
                return None;
            }
            let type_args = fresh_args(enums, &menu, &mut env.uni);
            let expected = payload.first().cloned().unwrap_or(Type::One).instantiate(&type_args);
            if let Some(actual) = check_expr(arg, enums, env, diags)
                && !fits_turning(env, &expected, &actual, arg)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`.{dtor}` carries a continuation of type {expected}; this has type \
                         {actual}"
                    ),
                    span: arg.span,
                });
            }
            Some(Type::Named(menu, type_args))
        }
        Expr::Project { base, key } => {
            let base_ty = check_expr(base, enums, env, diags)?;
            let base_ty = env.uni.apply(&base_ty);
            match key {
                // `base.i` — the i-th component along the tensor spine.
                slc_syntax::ast::ProjKey::Index(i) => {
                    let components = tensor_spine(&base_ty);
                    match components.get(*i) {
                        Some(ty) => {
                            env.dispatch.projections.insert(e.span, *i);
                            Some(ty.clone())
                        }
                        None => {
                            diags.push(Diagnostic {
                                message: format!(
                                    "`.{i}` is out of range for {base_ty}, which has {} \
                                     component(s)",
                                    components.len()
                                ),
                                span: e.span,
                            });
                            None
                        }
                    }
                }
                // `base.field` — a record field, resolved to its index.
                slc_syntax::ast::ProjKey::Field(name) => {
                    // `cfg.item` on a menu is a demand: the answer's type is
                    // the item's, and lowering cuts the menu against the
                    // request.
                    if let Type::Dual(inner) = &base_ty
                        && let Type::Named(menu, _) = inner.as_ref()
                        && enums.is_menu(menu)
                    {
                        let label = format!("{menu}::{name}");
                        let args = scrutinee_args(&base_ty).to_vec();
                        return match enums.variant(&label) {
                            Some((_, payload)) => {
                                env.dispatch.demands.insert(e.span, label);
                                Some(
                                    payload
                                        .first()
                                        .cloned()
                                        .unwrap_or(Type::One)
                                        .instantiate(&args)
                                        .dual(),
                                )
                            }
                            None => {
                                diags.push(Diagnostic {
                                    message: format!("`{menu}` has no item `{name}`"),
                                    span: e.span,
                                });
                                None
                            }
                        };
                    }
                    // A form is fed whole, never read a field at a time:
                    // from `-A ⅋ -B` there is no `-A` to be had, the way
                    // `A ⊗ B` yields its `A`. Say so, rather than leaving it
                    // at "not a record".
                    if let Type::Dual(inner) = &base_ty
                        && let Type::Named(form, _) = inner.as_ref()
                        && enums.is_form(form)
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "`{form}` is a form, and a form takes every field at once: send \
                                 it one, `{form} {{ … }} | …⟩`. A single field cannot be taken \
                                 out of it — the others would have to be invented"
                            ),
                            span: e.span,
                        });
                        return None;
                    }
                    let Type::Named(record_name, _) = &base_ty else {
                        diags.push(Diagnostic {
                            message: format!("`.{name}` needs a record; this has type {base_ty}"),
                            span: e.span,
                        });
                        return None;
                    };
                    match enums.records.get(record_name) {
                        Some(fields) => match fields.iter().position(|(f, _)| f == name) {
                            Some(index) => {
                                env.dispatch.projections.insert(e.span, index);
                                Some(fields[index].1.instantiate(scrutinee_args(&base_ty)))
                            }
                            None => {
                                diags.push(Diagnostic {
                                    message: format!("`{record_name}` has no field `{name}`"),
                                    span: e.span,
                                });
                                None
                            }
                        },
                        None => {
                            diags.push(Diagnostic {
                                message: format!("`{record_name}` is not a declared record"),
                                span: e.span,
                            });
                            None
                        }
                    }
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
        // A bundle of exits: every component is supplied, and whoever
        // holds it takes exactly one — the additive conjunction.
        Expr::Bundle(items) => items
            .iter()
            .map(|item| check_expr(item, enums, env, diags))
            .collect::<Option<Vec<_>>>()
            .map(|types| {
                types.into_iter().rev().reduce(|acc, ty| Type::With(Box::new(ty), Box::new(acc)))
            })?,
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
                if let Expr::Let { pattern, ty, value, body: None } = &expr.kind {
                    // A bodyless `let` scopes over the rest of the block; the
                    // binding itself is checked exactly as the expression form.
                    let binding_ty = check_let_binding(pattern, ty, value, enums, env, diags);
                    bind_let_pattern(pattern, binding_ty, value, enums, env);
                } else {
                    result = check_expr(expr, enums, env, diags);
                }
            }
            env.pop();
            result
        }
        // `a | b | c` — flow. The brackets say what the chain is, so
        // nothing is guessed: `⟨` makes the first stage a value, `⟩` makes
        // the last one consume, and every other step composes. A chain
        // that does not begin with a value denotes one that would, read as
        // `λx. x | …`, which gives composition its type for free —
        // `arrow(x, ⊥)` is a consumer, `arrow(x, B)` a function.
        Expr::Flow { stages, from_value, into_consumer } => {
            let types: Vec<Option<Type>> = stages
                .iter()
                .map(|stage| check_expr(stage, enums, env, diags).map(|ty| env.uni.apply(&ty)))
                .collect();
            // A chain composes when a function heads it; anything else
            // there is a value flowing in, which needs no mark. `⟨` says
            // "a value" for the one case that would otherwise compose: a
            // function handed on as a value.
            let opens = !from_value
                && types
                    .first()
                    .and_then(|ty| ty.as_ref())
                    .is_some_and(|ty| matches!(ty, Type::Par(..)));
            let entry = env.uni.fresh_var();
            let mut acc = if opens {
                entry.clone()
            } else {
                types[0].clone().unwrap_or_else(|| env.uni.fresh_var())
            };
            // What flows in, as it was written: an integer literal takes the
            // width the next stage requires, and only the first stage can be
            // one — everything later is the result of a step.
            let mut flowing: Option<&Expr> = (!opens).then(|| &stages[0].kind);
            let mut commuted_from: Option<usize> = None;
            let mut row_stage: Option<usize> = None;
            let mut swap: Option<slc_syntax::lower::Swap> = None;
            for (index, ty) in types.iter().enumerate().skip(usize::from(!opens)) {
                let last = index + 1 == types.len();
                let unknown = env.uni.fresh_var();
                let ty = ty.as_ref().unwrap_or(&unknown);
                let shape = flowing.unwrap_or(&e.kind);
                // A command takes two groups, so a chain hands it both:
                // what flows in is its values, and the rest of the chain —
                // the closing stage — is its menu of exits. The chain ends
                // there, in a call, and its type is `⊥`.
                if *into_consumer
                    && index + 2 == types.len()
                    && let Expr::Ident(name) = &stages[index].kind
                    && env.lookup(name).is_none()
                    && !env.traits.is_method(name)
                    && let Some(signature) = env.functions.get(name)
                    && signature.continuations.iter().any(|c| *c)
                    // Only a command: it answers `⊥`, so the chain ends
                    // in it. A negative function also carries a continuation
                    // but answers a consumer, and composes on — that is the
                    // commuted ⅋ reading below.
                    && signature.result.as_ref() == Some(&Type::Bottom)
                {
                    let (signature, _) = instantiate(signature, &mut env.uni);
                    let split = signature.continuations.iter().filter(|c| !**c).count();
                    let values = signature.params[..split]
                        .iter()
                        .rev()
                        .cloned()
                        .reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
                        .unwrap_or(Type::One);
                    let row = signature.params[split..]
                        .iter()
                        .rev()
                        .cloned()
                        .reduce(|acc, ty| Type::With(Box::new(ty), Box::new(acc)))
                        .unwrap_or(Type::One);
                    if !signature.builtin && !fits(env, &values, &acc, shape) {
                        let values = env.uni.apply(&values);
                        diags.push(Diagnostic {
                            message: format!(
                                "`{name}` takes {values}, and what flows in has type {acc}"
                            ),
                            span: stages[index].span,
                        });
                    }
                    if let Some(exits) = types[index + 1].as_ref()
                        && !signature.builtin
                        && !fits(env, &row, exits, &stages[index + 1].kind)
                    {
                        let row = env.uni.apply(&row);
                        diags.push(Diagnostic {
                            message: format!(
                                "`{name}` offers the exits {row}, and this menu has type {exits}"
                            ),
                            span: stages[index + 1].span,
                        });
                    }
                    row_stage = Some(index);
                    acc = Type::Bottom;
                    break;
                }
                // A declared function is checked through its signature, as
                // a call is: `x | f` *is* `f(x)`, so its parameters pack the
                // same way and its bounds are discharged the same way.
                if !(last && *into_consumer)
                    && let Expr::Ident(name) = &stages[index].kind
                    && env.lookup(name).is_none()
                    // A trait method dispatches instead; that is the arm below.
                    && !env.traits.is_method(name)
                    && let Some(signature) = env.functions.get(name)
                    && {
                        // Only when its parameters take what flows in: a
                        // negative function may instead read the other way
                        // round, and that is the general arm below.
                        // The signature's template variables are fresh per
                        // call, so probe with an instantiated copy.
                        let mut probe = env.uni.clone();
                        let fresh = instantiate(signature, &mut probe).0;
                        let packed = fresh
                            .params
                            .iter()
                            .rev()
                            .cloned()
                            .reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
                            // No parameters is the empty product: `(,) | f`
                            // is how a nullary declaration is called.
                            .unwrap_or(Type::One);
                        let piecewise = fits_piecewise(&probe, &fresh.params, &acc, shape);
                        let probe = Env { uni: probe, ..env.clone() };
                        signature.builtin
                            || piecewise
                            || would_fit(&probe, &packed, &acc, Some(shape))
                            || (flowing.is_some() && commutes(&probe.uni, &packed, &acc))
                    }
                {
                    let (signature, seen) = instantiate(signature, &mut env.uni);
                    let packed = signature
                        .params
                        .iter()
                        .rev()
                        .cloned()
                        .reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
                        .unwrap_or(Type::One);
                    // A tuple written in place is checked component by
                    // component, so an integer literal still takes the
                    // width its slot requires.
                    let written = match shape {
                        Expr::Pair(items) if items.len() == signature.params.len() => Some(items),
                        _ => None,
                    };
                    let components = tensor_spine(&env.uni.apply(&acc));
                    let piecewise = written.is_some_and(|items| {
                        items.len() == components.len()
                            && items.iter().zip(components.iter()).zip(signature.params.iter()).all(
                                |((item, actual), param)| fits_turning(env, param, actual, item),
                            )
                    });
                    let fitted = piecewise
                        || signature.builtin
                        || match flowing {
                            // What flows in is a written value, so it can be
                            // turned around where it stands.
                            Some(_) if index == 1 => fits_turning(env, &packed, &acc, &stages[0]),
                            _ => fits(env, &packed, &acc, shape),
                        };
                    if !fitted {
                        let packed = env.uni.apply(&packed);
                        diags.push(Diagnostic {
                            message: format!(
                                "`{name}` takes {packed}, and what flows in has type {acc}"
                            ),
                            span: stages[index].span,
                        });
                    }
                    if !signature.bounds.is_empty() {
                        let bounds = signature
                            .bounds
                            .iter()
                            .filter_map(|(position, trait_name)| {
                                seen.get(position).map(|var| (trait_name.clone(), var.clone()))
                            })
                            .collect();
                        env.pending_dicts.push(crate::env::PendingDicts {
                            span: stages[index].span,
                            callee: name.clone(),
                            bounds,
                        });
                    }
                    acc = signature.result.map(|ty| env.uni.apply(&ty)).unwrap_or(Type::One);
                    flowing = None;
                    continue;
                }
                // A trait method takes what flows in as its receiver, and
                // dispatches on it exactly as a call would.
                if !(last && *into_consumer)
                    && let Expr::Ident(name) = &stages[index].kind
                    && env.traits.is_method(name)
                    && let Some(result) =
                        check_method_stage(name, &acc, stages[index].span, enums, env, diags)
                {
                    acc = result;
                    flowing = None;
                    continue;
                }
                // The closing stage consumes; every other one is a function.
                if last && *into_consumer {
                    // The orientation rule: a consumer stands only at the
                    // right end, so one arriving from the left is the
                    // mistake. A function and codata are negative *values*
                    // and flow in like any other.
                    if acc.is_negative()
                        && !matches!(acc, Type::Par(..))
                        && !contains_var(&acc)
                        && !enums.is_negative_value(&acc)
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "the left of `|` is the value side, and this has type {acc}; \
                                 pass a continuation as an argument instead"
                            ),
                            span: stages[index].span,
                        });
                        return Some(Type::Bottom);
                    }
                    let expects = ty.dual();
                    // The ⊥/1 corner: `-⊥` resolves to `1`, so the
                    // idiomatic `⟨(,) | k⟩` is unit meeting unit.
                    let units = acc == Type::One && ty == &Type::One;
                    let before = env.uni.clone();
                    if !units && !fits(env, &expects, &acc, shape) {
                        // The forward reading failed; whatever it bound is
                        // undone before the mirrored one is tried.
                        env.uni = before;
                        match commute(env, &expects, &acc) {
                            Some(turned) => swap = Some(turned),
                            None => {
                                let expects = env.uni.apply(&expects);
                                diags.push(Diagnostic {
                                    message: format!(
                                        "this consumer takes {expects}, and what flows in has \
                                         type {acc}"
                                    ),
                                    span: stages[index].span,
                                });
                            }
                        }
                    }
                    acc = Type::Bottom;
                    continue;
                }
                // A function: what flows in is its argument, its result
                // flows on. `⅋` is commutative, so a stage `A ⅋ B` reads
                // both ways — `dual(A) → B` and `dual(B) → A` — and the two
                // styles meet here: `area: Shape -> i64` and
                // `area_of(out: -i64) <- Shape` are one type, so either
                // stands in a pipeline. What flows in picks the reading,
                // and where both fit they agree.
                // A declared callee with a value group has had its chance in
                // the signature arm, which supplies the whole group or
                // nothing. Reaching here means what flows in is only part of
                // it: `⅋` is associative, so the callee's type presents its
                // first parameter alone, and reading it that way would apply
                // `route` to `"high"` and leave the rest for later — a
                // partial application the calling convention does not have,
                // since a group is bound as one argument. A negative `fn`,
                // whose only group is its row, still reads the mirrored way
                // below.
                if let Expr::Ident(name) = &stages[index].kind
                    && env.lookup(name).is_none()
                    && let Some(signature) = env.functions.get(name)
                    && signature.continuations.iter().any(|c| !*c)
                    && !signature.builtin
                {
                    let values: Vec<String> = signature
                        .params
                        .iter()
                        .zip(&signature.continuations)
                        .filter(|(_, is_row)| !**is_row)
                        .map(|(ty, _)| ty.to_string())
                        .collect();
                    let group = match values.as_slice() {
                        [one] => one.clone(),
                        many => format!("({})", many.join(" ⊗ ")),
                    };
                    let is_command = signature.continuations.iter().any(|c| *c);
                    // The values may all be there, with only the exits
                    // missing: then the chain is what is short, not the group.
                    let values_fit = {
                        let mut probe = env.uni.clone();
                        let (fresh, _) = instantiate(signature, &mut probe);
                        let packed = fresh
                            .params
                            .iter()
                            .zip(&fresh.continuations)
                            .filter(|(_, is_row)| !**is_row)
                            .map(|(ty, _)| ty.clone())
                            .rev()
                            .reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
                            .unwrap_or(Type::One);
                        let probe = Env { uni: probe, ..env.clone() };
                        would_fit(&probe, &packed, &acc, Some(shape))
                    };
                    let message = if is_command && values_fit {
                        format!(
                            "`{name}` is a command: after its values it takes its menu of \
                             exits, so the chain closes on them — `… | {name} | (…)⟩`"
                        )
                    } else {
                        let exits = if is_command {
                            " and then its menu of exits, closing the chain"
                        } else {
                            ""
                        };
                        format!(
                            "`{name}` takes its whole value group, {group}{exits}; what flows \
                             in has type {acc}, and a call is not applied to part of a group"
                        )
                    };
                    diags.push(Diagnostic { message, span: stages[index].span });
                    return None;
                }
                let left = env.uni.fresh_var();
                let right = env.uni.fresh_var();
                let par = Type::Par(Box::new(left.clone()), Box::new(right.clone()));
                if env.uni.unify(ty, &par).is_err() {
                    diags.push(Diagnostic {
                        message: format!(
                            "a step composes, so this stage is a function; it has type {ty}. \
                             To deliver to it instead, close the chain: `… | consumer⟩`"
                        ),
                        span: stages[index].span,
                    });
                    return None;
                }
                let (left, right) = (env.uni.apply(&left), env.uni.apply(&right));
                let forward = left.dual();
                if fits(env, &forward, &acc, shape) {
                    acc = env.uni.apply(&right);
                } else {
                    let commuted = right.dual();
                    commuted_from.get_or_insert(index);
                    if !fits(env, &commuted, &acc, shape) {
                        let forward = env.uni.apply(&forward);
                        diags.push(Diagnostic {
                            message: format!(
                                "this stage takes {forward}, and what flows in has type {acc}"
                            ),
                            span: stages[index].span,
                        });
                    }
                    acc = env.uni.apply(&left);
                }
                flowing = None;
            }
            // `⟩` is syntax, but "does a function head this chain?" is not,
            // so lowering is told.
            env.dispatch.flows.insert(
                e.span,
                slc_syntax::lower::FlowShape {
                    eta: opens,
                    cut: *into_consumer,
                    commuted_from,
                    row_stage,
                    swap,
                },
            );
            Some(if opens { Type::arrow(env.uni.apply(&entry), acc) } else { acc })
        }
        Expr::Mu { continuation_params, body, .. } => {
            env.push();
            let mut captured_types = Vec::new();
            for p in continuation_params {
                let ty = match (&p.ty, p.name()) {
                    (Some(ty), _) => resolve_in_body(ty, env, enums),
                    // Nothing was written, so the body says it.
                    (None, Some(name)) => infer_param_type(name, body, enums, env),
                    (None, None) => None,
                }
                .unwrap_or_else(|| env.uni.fresh_var());
                bind_match_pattern(&p.pattern, &ty, enums, env);
                captured_types.push(Some(ty));
            }
            let result = check_expr(body, enums, env, diags);
            env.pop();
            // A `mu` captures the ambient continuation, so its value is
            // whatever that continuation receives: `mu A { k <= … }` has
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
    fn a_command_body_must_be_bottom() {
        // A bare value reaches no continuation.
        let diags = check("command bad(x: +i32) | (k: -i32) { x }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must reach a continuation")), "{diags:?}");
        // An `if` with no `else` falls through on the false path.
        let diags =
            check("command bad(x: +i32) | (k: -i32) { if eq(x, 0) { x | k⟩ } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must reach a continuation")), "{diags:?}");
    }

    #[test]
    fn a_command_may_leave_a_continuation_unused() {
        // The core is classical: reaching one continuation is enough, so a
        // declared continuation the body never triggers is not an error.
        assert!(
            check("command f(x: +i32) | (ok: -i32 & err: -i32) { x | ok⟩ }").is_ok(),
            "dropping a continuation should be allowed"
        );
    }

    #[test]
    fn projection_resolves_and_range_checks() {
        // A tuple component and a record field both check.
        assert!(
            check(
                "data P { x: +i64, y: +i64 }
                 fn f(p: +P) -> i64 { p.x + p.y }
                 fn g(t: (+i64 ⊗ (+i64 ⊗ +i64))) -> i64 { t.0 + t.2 }"
            )
            .is_ok()
        );
        // Out of range.
        let diags = check("fn f(t: (+i64 ⊗ +i64)) -> i64 { t.5 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("out of range")), "{diags:?}");
        // Unknown field.
        let diags = check("data P { x: +i64 } fn f(p: +P) -> i64 { p.y }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no field `y`")), "{diags:?}");
    }

    #[test]
    fn record_literal_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "data Direction { left: i64, right: i64 }
                 fn use_it(d: Direction) -> i64 { 0 }
                 fn f() -> i64 { Direction { left: 1, right: 2 } | use_it }"
            )
            .is_ok()
        );

        let missing = check(
            "data Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            missing.iter().any(|d| d.message.contains("the literal writes `left`")),
            "{missing:?}"
        );

        let reordered = check(
            "data Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { right: 2, left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            reordered.iter().any(|d| d.message.contains("the literal writes `right`, `left`")),
            "{reordered:?}"
        );
    }

    #[test]
    fn record_pattern_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "data D { left: i64, right: i64 }
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
            "data D { left: i64, right: i64 }
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
    fn record_pattern_field_types_are_checked() {
        let diags = check(
            "data D { left: i64, right: i64 }
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
    fn record_pattern_cannot_match_another_type() {
        let diags = check(
            "data D { left: i64 }
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
    fn record_literal_field_types_are_checked() {
        let diags = check(
            "data Direction { left: i64, right: i64 }
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
            diags.iter().any(|d| d.message.contains("`Nope` is not a declared record")),
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
            "data S { a: i32 } fn f() -> i32 { S }",
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
                 fn k(ok: -i64 & absent: -i64) <- R {
                     select R {
                         Some(value) => value | ok⟩,
                         None => 0 | absent⟩,
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
             fn k(ok: -i64 & absent: -i64) <- R {
                 select R {
                     Some => 0 | ok⟩,
                     None => 0 | absent⟩,
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
                     None(value) => 0 | absent⟩,
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
                && d.message.contains("value | k⟩")),
            "{diags:?}"
        );
        assert!(check("command route(x: +i32) | (k: -i32) { x | k⟩ }").is_ok());
    }

    #[test]
    fn exit_is_a_continuation() {
        let diags = check("command f | (out: -i32) { out(0) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`out` is a consumer of type -i32")),
            "{diags:?}"
        );
        assert!(check("command f | (out: -i32) { 0 | out⟩ }").is_ok());
    }

    #[test]
    fn a_cut_is_a_command() {
        // A cut has type ⊥: it produces nothing and control does not return,
        // so a branch that ends in one leaves the `if` type to the other.
        let ok = check(
            "fn parse(input: +String, err: -String) -> i64 {
                 if str_len(input) > 0 { 1 } else { \"empty\" | err⟩ }
             }",
        );
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn a_cut_needs_dual_sides() {
        // Two values of the same positive type do not interact.
        let diags = check("fn f(x: +i32, y: +i32) -> i32 { x | y⟩ }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("what flows in has type")), "{diags:?}");
    }

    #[test]
    fn a_cut_checks_what_the_consumer_accepts() {
        let diags = check("command route(x: +String) | (k: -i32) { x | k⟩ }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("this consumer takes +i32")
                && d.message.contains("what flows in has type +String")),
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
                         Red => 0 | return⟩,
                         Green => 1 | return⟩,
                     }
                 }
                 fn main() -> i64 {
                     mu i64 { answer <= Color::Green | code | answer⟩ }
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_accepts_the_declared_row() {
        assert!(
            check(
                "command route(x: +i32) | (k: -i32) { x | k⟩ }
                 fn main() -> i32 { mu i32 { out <= 1 | route | out⟩ } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_rejects_an_incompatible_continuation_type() {
        let diags = check(
            "command route(x: +i32) | (k: -i32) { x | k⟩ }
             fn main() -> i32 { mu bool { out <= route(1, out) } }",
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
            "command route(a: -i32, b: -bool) | (c: -i32 & d: -bool) { 0 | c⟩ }
             command caller | (first: -i32 & second: -bool) { route(0, true, second, first) }",
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
            "command route(x: +i32) | (k: -i32) { x | k⟩ }
             fn main() -> i32 { mu i32 { out <= route(1, out, out) } }",
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
                    Red => 0 | return⟩,
                    Green => 1 | return⟩,
                    Blue => 2 | return⟩,
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
                    Red => 0,
                    Green => 1 | return⟩,
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
                    (a, b) => 0 | return⟩,
                    Green => 1 | return⟩,
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
             command main | (exit: -i32) / {IO} { println(wants((,))); 0 | exit⟩ }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type 1")),
            "unit fit everything once: {diags:?}"
        );
    }

    #[test]
    fn prelude_shaped_unit_and_bottom_declarations_are_builtin_aliases() {
        assert!(
            check(
                "data Unit {}
                 form Bottom {}
                 fn named_unit() -> Unit { (,) }
                 fn symbolic_unit() -> unit { Unit {} }
                 fn stop(exit: -i32) -> Bottom {
                     select { Bottom {} => 0 | exit⟩ }
                 }
                 command absorb | (never: -Bottom) { (,) | never⟩ }
                 command halt | (exit: -i32) { 0 | exit⟩ }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_differently_shaped_unit_shadow_stays_nominal() {
        let diags = check("data Unit { value: i64 } fn not_unit() -> Unit { (,) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("initializer") || d.message.contains("body")),
            "{diags:?}"
        );

        let diags = check("data Bottom {} command halt -> Bottom { (,) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("not the nullary prelude form")),
            "{diags:?}"
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
                 command main | (exit: -i32) / {IO} {
                     let r = handle u() { ask(): resume => 1000 + resume(7), return(n) => n };
                     println(r); 0 | exit⟩
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
                 command main | (exit: -i32) / {IO} {
                     let r = handle f() {
                         c(): resume => resume(true) + resume(false),
                         return(n) => n,
                     };
                     println(r); 0 | exit⟩
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
                 impl Show for i64 { fn show(self: +i64) -> String { self | int_to_str } }
                 fn label<T: Show>(x: +T) -> String { x | show }
                 command main | (exit: -i32) / {IO} { 1 | label | println; 0 | exit⟩ }"
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
             impl Show for i64 { fn show(self: +i64) -> String { self | int_to_str } }
             command main | (exit: -i32) / {IO} { 1 | show | println; 0 | exit⟩ }",
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
             impl Show for i64 { fn show(self: +i64) -> String { self | int_to_str } }
             fn label<T: Show>(x: +T) -> String { x | show }
             command main | (exit: -i32) / {IO} { 1 | label | println; 0 | exit⟩ }",
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
            poly.calls.values().any(|dicts| dicts.iter().any(|d| d.name.contains("Show"))),
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
                 fn emit<T: Show>(out: -String & v: +T) <- i64 { show(v) | out⟩ }"
            )
            .is_ok()
        );
        let diags = check(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             fn emit<T>(out: -String & v: +T) <- i64 { show(v) | out⟩ }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("not known to satisfy")), "{diags:?}");
    }

    #[test]
    fn a_method_with_no_impl_is_rejected() {
        let diags = check(
            "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { int_to_str(self) } }
             command main | (exit: -i32) / {IO} { println(show(true)); 0 | exit⟩ }",
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
                "command main | (exit: -i32) / {IO} {
                     let f = fn(x) { x };
                     println(f(1) + 1);
                     println(str_len(f(\"s\")));
                     let alias = f;
                     println(alias(true));
                     0 | exit⟩
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
            "command main | (exit: -i32) / {IO} {
                 let g = mu { k <= ⟨fn(x) { x } | k⟩ };
                 println(g(1) + 1);
                 println(str_len(g(\"s\")));
                 0 | exit⟩
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");

        // An application is not a value either: it may run a command, and
        // its result may hold a captured continuation.
        let diags = check(
            "fn id<T>(x: T) -> T { x }
             command main | (exit: -i32) / {IO} {
                 let h = id(fn(x) { x });
                 println(h(1) + 1);
                 println(str_len(h(\"s\")));
                 0 | exit⟩
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
                "command main | (exit: -i32) / {IO} {
                     let fresh = fn(u) { mu { k <= ⟨fn(x) { x } | k⟩ } };
                     println(fresh((,))(1) + 1);
                     println(str_len(fresh((,))(\"s\")));
                     0 | exit⟩
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
            "command main | (exit: -i32) / {IO} {
                 let g = fn(x) { x };
                 println(g(1) + str_len(g(1)));
                 0 | exit⟩
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("expected +String")),
            "closure calls went unchecked once: {diags:?}"
        );

        // The body constrains the parameter, and the call site honors it.
        let diags = check(
            "command main | (exit: -i32) / {IO} {
                 println(fn(x) { x + 1 }(\"not a number\"));
                 0 | exit⟩
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
                 command main | (exit: -i32) / {IO} {
                     (42 | id) + 1 | println;
                     \"each call its own T\" | id | str_len | println;
                     0 | exit⟩
                 }"
            )
            .is_ok()
        );

        // Within one call, T is one type.
        let diags = check(
            "fn id<T>(x: T) -> T { x }
             command main | (exit: -i32) / {IO} { println(str_len(id(42))); 0 | exit⟩ }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");
    }

    #[test]
    fn a_body_produces_what_the_declaration_promises() {
        let diags = check("fn f() -> i64 { \"not an i64\" }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the declaration says +i64")), "{diags:?}");
        // An integer literal still adapts to the declared width.
        assert!(check("fn f() -> i32 { 0 }").is_ok());
        // A body that ends in a cut produces nothing, and promises nothing.
        assert!(check("fn f(k: -i64) <- i64 { 1 | k⟩ }").is_ok());
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
    fn a_call_is_not_applied_to_part_of_its_group() {
        // A command given only some of its values was accepted — its
        // `⅋`-nested type presented the first parameter alone — and then
        // crashed at run time, where the group is bound as one argument.
        let route = "command route(tag: String, x: i64) | (k: i64) { x | k⟩ }\n";
        for body in [r#"let h = "high" | route; 0 | exit⟩"#, r#""high" | route; 0 | exit⟩"#] {
            let diags = check(&format!("{route}command main | (exit: -i32) / {{IO}} {{ {body} }}"))
                .unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains("not applied to part of a group")),
                "{body}: {diags:?}"
            );
        }
        // A positive function likewise — and one named like a builtin, which
        // used to inherit every builtin exemption by name and slip past.
        let diags = check(
            "fn add(a: i64, b: i64) -> i64 { a + b }
             command main | (exit: -i32) / {IO} { let inc = 1 | add; 0 | exit⟩ }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("not applied to part of a group")),
            "{diags:?}"
        );
        // All the values and no exits is a command short of its chain, and
        // says so.
        let diags = check(
            "command one(x: i64) | (k: i64) { x | k⟩ }
             command main | (exit: -i32) / {IO} { let h = 1 | one; 0 | exit⟩ }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`one` is a command")), "{diags:?}");
        // The whole group still calls, and a negative function still reads
        // the mirrored way round.
        assert!(
            check(&format!(
                "{route}command main | (exit: -i32) / {{IO}} {{
                     mu i64 {{ k <= (\"high\", 7) | route | k⟩ }} | println; 0 | exit⟩ }}"
            ))
            .is_ok()
        );
        assert!(
            check(
                "fn plus_one(out: i64) <- i64 { select i64 { n => n + 1 | out⟩ } }
                 fn double(n: i64) -> i64 { n * 2 }
                 command main | (exit: -i32) / {IO} {
                     mu i64 { out <= 20 | plus_one | double | out⟩ } | println; 0 | exit⟩ }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_value_is_cut_into_a_consumer_at_the_mirrored_spelling_of_its_type() {
        // `+String ⅋ -i64` into a slot of `-i64 ⅋ +String`: one type.
        assert!(
            check(
                "menu Deliver { deliver: (i64 -> String) }
                 fn deliver_i64(out: String) <- i64 { select i64 { n => n | int_to_str | out⟩ } }
                 fn delivers() -> Deliver { mu Deliver { deliver <= ⟨deliver_i64 | deliver⟩ } }"
            )
            .is_ok()
        );
        // Inside a constructor there is no one value to turn around, so a
        // component's spelling still has to match.
        assert!(
            check(
                "fn deliver_i64(out: String) <- i64 { select i64 { n => n | int_to_str | out⟩ } }
                 fn f() -> i64 { let p: ((i64 -> String) ⊗ i64) = (deliver_i64, 1); 0 }"
            )
            .is_err()
        );
    }

    #[test]
    fn a_nullary_declaration_is_called_with_the_unit() {
        // No parameters is the empty product, so `(,)` is what flows in.
        assert!(
            check(
                "fn answer() -> i64 { 42 }
                 command main | (exit: -i32) / {IO} { (,) | answer | println; 0 | exit⟩ }"
            )
            .is_ok()
        );
        // A value flowing into one is not a call: nothing takes it.
        assert!(
            check(
                "fn answer() -> i64 { 42 }
                 command main | (exit: -i32) / {IO} { 1 | answer | println; 0 | exit⟩ }"
            )
            .is_err()
        );
    }

    #[test]
    fn a_bound_on_a_negative_function_is_discharged_by_the_cut() {
        let prelude = "trait Show { fn show(self: +Self) -> String; }
             impl Show for i64 { fn show(self: +i64) -> String { \"n\" } }
             impl Show for bool { fn show(self: +bool) -> String { \"b\" } }
             fn emit<T: Show>(out: -String) <- T { fn(x: T) { x | show | out⟩ } }\n";
        // Nothing the call receives mentions T; the cut fixes it, at two
        // different types in the same declaration.
        assert!(
            check(&format!(
                "{prelude} command main | (exit: -i32) / {{IO}} {{
                     mu String {{ s <= 42 | emit | s⟩ }} | println;
                     mu String {{ s <= true | emit | s⟩ }} | println;
                     0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
        // A type with no impl is still refused.
        let diags = check(&format!(
            "{prelude} command main | (exit: -i32) / {{IO}} {{
                 println(mu String {{ s <= \"text\" | emit(s)⟩ }}); 0 | exit⟩
             }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no `impl Show for String`")), "{diags:?}");
    }

    #[test]
    fn a_trait_method_may_consume_self() {
        // A negative method has no `self` parameter: its Self is what it
        // consumes, and the cut says which impl runs.
        let prelude = "trait Deliver { fn deliver(out: -String) <- Self; }
             impl Deliver for i64 {
                 fn deliver(out: -String) <- i64 { fn(n: +i64) { \"i\" | out⟩ } }
             }
             impl Deliver for bool {
                 fn deliver(out: -String) <- bool { fn(b: +bool) { \"b\" | out⟩ } }
             }\n";
        assert!(
            check(&format!(
                "{prelude} command main | (exit: -i32) / {{IO}} {{
                     println(mu String {{ s <= 42 | deliver(s)⟩ }});
                     println(mu String {{ s <= true | deliver(s)⟩ }});
                     0 | exit⟩
                 }}"
            ))
            .is_ok()
        );
        let diags = check(&format!(
            "{prelude} command main | (exit: -i32) / {{IO}} {{
                 println(mu String {{ s <= \"text\" | deliver(s)⟩ }}); 0 | exit⟩
             }}"
        ))
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("no `impl Deliver for String`")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_body_annotation_names_the_declarations_type_parameter() {
        // `T` written inside a body is the declaration's rigid `T`, bounds
        // and all — not a fresh name that merely looks alike.
        assert!(
            check(
                "trait Show { fn show(self: +Self) -> String; }
                 impl Show for i64 { fn show(self: +i64) -> String { \"n\" } }
                 fn wrap<T: Show>(x: T) -> String { let f = fn(y: T) { show(y) }; f(x) }"
            )
            .is_ok()
        );
        assert!(
            check(
                "trait Show { fn show(self: +Self) -> String; }
                 impl Show for i64 { fn show(self: +i64) -> String { \"n\" } }
                 fn annotated<T: Show>(x: T) -> String { let y: T = x; show(y) }"
            )
            .is_ok()
        );
        // The negative shape's body checks on its own too.
        assert!(
            check(
                "trait Show { fn show(self: +Self) -> String; }
                 impl Show for i64 { fn show(self: +i64) -> String { \"n\" } }
                 fn emit<T: Show>(out: -String) <- T { fn(x: T) { show(x) | out⟩ } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_consumer_travels_bare() {
        // A continuation is a value: it passes as an ordinary argument and
        // sits in bindings without any box.
        assert!(check("fn hold(k: -i64) -> ⊥ { 1 | k⟩ }").is_ok());
    }

    #[test]
    fn the_cut_stays_oriented() {
        // A raw consumer is a value everywhere except the left of a cut:
        // there, involution would let any positive pass for a consumer of
        // consumers, and the machine only runs an oriented cut.
        let diags = check("fn f(k: -i64, target: -i64) -> ⊥ { k | target⟩ }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("the left of `|` is the value side")),
            "{diags:?}"
        );
    }

    #[test]
    fn double_negation_collapses() {
        // With no shifts, dual is an involution on the nose: `-(-T)` *is*
        // `T`, and double-negation elimination is the identity function.
        assert!(
            check(
                "fn dne<T>(t: -(-T)) -> T { t }
                 command main | (exit: -i32) / {IO} { 42 | dne | println; 0 | exit⟩ }",
            )
            .is_ok()
        );
    }

    #[test]
    fn value_arguments_are_checked_against_the_declaration() {
        let diags = check(
            "fn f(x: +String) -> i64 { 0 }
             command main | (exit: -i32) / {IO} { println(f(42)); 0 | exit⟩ }",
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
                     select { Red => 0 | return⟩, Green => 1 | return⟩ }
                 }"
            )
            .is_ok()
        );

        // Exhaustiveness of an inferred type is checked in `exhaustive`.
    }

    #[test]
    fn a_select_in_a_negative_fn_takes_the_type_it_consumes() {
        // Nothing in `n <= …` names a type, but the declaration already did.
        assert!(check("fn twice(out: -i64) <- +i64 { select { n => (n * 2) | out⟩ } }").is_ok());
        let diags = check("fn twice(out: -String) <- +i64 { select { n => str_len(n) | out⟩ } }")
            .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );

        // Outside one, with no arm naming a type, it has to be written.
        let diags = check(
            "command main | (exit: -i32) / {IO} {
                 let show = select { n => println(n) };
                 42 | show⟩;
                 0 | exit⟩
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no arm names a type")), "{diags:?}");
    }

    #[test]
    fn a_local_mu_takes_its_parameter_type_from_the_body() {
        // `k` is handed to a slot `__read_file` declares, so it is `-String`,
        // and the `mu` therefore produces a `+String`.
        let diags = check(
            "command main | (exit: -i32) / {IO} {
                 let complain = select { m => { println(m); 1 | exit⟩ } };
                 let text = mu { k <= __read_file(\"in\", k, complain) };
                 println(text + 1);
                 0 | exit⟩
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("+String and +i64")),
            "the inferred type should reach the use: {diags:?}"
        );

        // A cut says it just as well: `42 | k` makes `k` a consumer of i64.
        let diags = check(
            "command main | (exit: -i32) / {IO} {
                 let answer = mu { k <= 42 | k⟩ };
                 println(str_len(answer));
                 0 | exit⟩
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
            check("fn show(out: -String) <- +i64 { select +i64 { n => int_to_str(n) | out⟩ } }")
                .is_ok()
        );

        let diags =
            check("fn show(out: -String) <- +i64 { select +i64 { n => str_len(n) | out⟩ } }")
                .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );
    }
}
