//! Expression type checking for the ergonomic surface syntax.
//!
//! This pass is intentionally concrete: it validates boolean operators,
//! numeric/comparison operators, indexing, patterns, and annotated
//! bindings using known literal and declaration types. It is not a
//! replacement for the core inference pass; it catches the surface-syntax
//! mistakes that used to appear only at runtime.

use crate::declarations::{Declarations, enum_types};
use crate::env::{Env, constant_types};
use crate::signatures::{FunctionSignature, function_types, instantiate};
use slc_core::types::{Base, Type};
use slc_core::typing::contains_var;
use slc_syntax::ast::{Decl, Expr, Named, Node, ParamPolarity, Program, TraitBound, TypeExpr};
use slc_syntax::lower::lower_type;
use slc_syntax::token::Span;
use slc_syntax::traits::{TraitInfo, assoc_type_name, parse_assoc_type_name};
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
    check_program_with_rows(p, traits).map(|(dispatch, _)| dispatch)
}

/// Check the program, and hand back beside what lowering needs what the
/// rows in its types refuse (`docs/design-notes/rows-in-types.md`): the
/// effects a declaration performs beyond its row, and the rows an argument
/// or an arm does not fit.
pub fn check_program_with_rows(
    p: &Program,
    traits: &TraitInfo,
) -> Result<(slc_syntax::lower::DispatchInfo, Vec<Diagnostic>), Vec<Diagnostic>> {
    let constants = constant_types(p);
    let mut enums = enum_types(p);
    for (trait_name, items) in &traits.assocs {
        let params = traits.params_of(trait_name).len();
        for item in items {
            enums.note_projection(trait_name, item, params);
        }
    }
    let functions = function_types(p, &enums);
    let mut diags = Vec::new();
    let mut env = Env::root(&constants, &functions, traits);
    env.elaboration_origins.clone_from(&traits.span_origins);
    env.declarations = Some(&enums);
    env.uni.set_negative_decls(enums.menus.iter().chain(enums.forms.iter()).cloned());
    env.uni.set_latent_decls(enums.menus.iter().chain(enums.forms.iter()).map(|name| {
        (
            name.clone(),
            enums.latent_rows.get(name).cloned().unwrap_or_default(),
            enums.latent_row_param(name),
        )
    }));
    check_declared_types(p, &enums, &mut diags);
    for declaration in &p.decls {
        if let Decl::Data { name, .. }
        | Decl::Enum { name, .. }
        | Decl::Menu { name, .. }
        | Decl::Form { name, .. } = &declaration.kind
            && matches!(name.as_str(), "Handler" | "Delayed")
        {
            diags.push(Diagnostic {
                message: format!("`{name}` is a built-in type and cannot be redeclared"),
                span: declaration.span,
            });
        }
    }
    check_trait_signatures(traits, &enums, &mut env, &mut diags);
    check_super_obligations(traits, &enums, &mut env, &mut diags);
    check_assoc_bindings(traits, &enums, &mut env, &mut diags);
    for d in &p.decls {
        let mut preview = env.clone();
        check_decl(d, &enums, &mut preview, &mut Vec::new());
        for (span, ty) in &preview.expr_types {
            if let Some(polarity @ (ParamPolarity::Positive | ParamPolarity::Negative)) =
                type_polarity(&preview.uni.apply(ty), &preview)
            {
                env.polarity_hints.insert(*span, polarity);
            }
        }
        check_decl(d, &enums, &mut env, &mut diags);
    }
    // A row variable in a menu's or form's own row is one of its row
    // parameters: declared, without a sign, beside its type parameters.
    for d in &p.decls {
        if let Decl::Menu { name, effects, .. } | Decl::Form { name, effects, .. } = &d.kind
            && let Some(tail) = effects.tails.first()
            && enums.latent_row_param(name).is_none()
        {
            env.row_diagnostics.push(Diagnostic {
                message: format!(
                    "`..{tail}` in `{name}`'s row is not one of its row parameters; declare it \
                     without a sign: `{name}<{tail}>`"
                ),
                span: d.span,
            });
        }
    }
    resolve_pending_injections(&mut env, &mut diags);
    resolve_pending_pars(&mut env, &mut diags);
    for diagnostic in diags.iter_mut().chain(env.row_diagnostics.iter_mut()) {
        if let Some(origin) = env.elaboration_origins.get(&diagnostic.span) {
            diagnostic.span = *origin;
        }
    }
    if diags.is_empty() {
        Ok((std::mem::take(&mut env.dispatch), std::mem::take(&mut env.row_diagnostics)))
    } else {
        Err(diags)
    }
}

/// Check that `target` satisfies `trait_name` applied to `args`: a ground
/// type must have an impl; a bound rigid variable is covered by the enclosing
/// declaration; an unsolved variable at a monomorphic call cannot be discharged.
fn discharge_bound(
    trait_name: &str,
    target: &Type,
    args: &[Type],
    callee: &str,
    span: Span,
    env: &Env,
    diags: &mut Vec<Diagnostic>,
) {
    let target = normalize(&env.uni.apply(target), env);
    let args: Vec<Type> = args.iter().map(|arg| normalize(&env.uni.apply(arg), env)).collect();
    if let Type::Var(v) = &target {
        if env.bounds.iter().any(|bound| {
            bound.var == *v && bound.trait_name == trait_name && types_agree(&bound.args, &args)
        }) {
            return;
        }
        diags.push(Diagnostic {
            message: format!(
                "`{callee}` needs `{}` for a type parameter, but the caller's type is not \
                 known to satisfy it",
                trait_applied(trait_name, &args)
            ),
            span,
        });
        return;
    }
    if let Err(message) = env.traits.select(trait_name, &target, &args) {
        diags.push(Diagnostic { message, span });
    }
}

/// A bound's pins, once its parameter is a real type or another parameter
/// that pins the same associated type.
fn check_pins(
    bound: &crate::env::PendingBound,
    target: &Type,
    args: &[Type],
    callee: &str,
    span: Span,
    env: &Env,
    diags: &mut Vec<Diagnostic>,
) {
    if bound.pins.is_empty() {
        return;
    }
    let target = normalize(&env.uni.apply(target), env);
    let args: Vec<Type> = args.iter().map(|arg| normalize(&env.uni.apply(arg), env)).collect();
    if let Type::Var(v) = &target {
        if !env.uni.is_rigid(*v) {
            return;
        }
        let have = env
            .bounds
            .iter()
            .find(|scope| {
                scope.var == *v
                    && scope.trait_name == bound.trait_name
                    && types_agree(&scope.args, &args)
            })
            .map(|scope| scope.pins.clone());
        let Some(have) = have else { return };
        for (item, expected) in &bound.pins {
            let Some((_, pinned)) = have.iter().find(|(name, _)| name == item) else {
                diags.push(Diagnostic {
                    message: format!(
                        "`{callee}` needs `{}::{item}` to be {}, and this type parameter does \
                         not pin it",
                        bound.trait_name,
                        normalize(expected, env)
                    ),
                    span,
                });
                continue;
            };
            let pinned = normalize(pinned, env);
            let expected = normalize(expected, env);
            if !same_type(env, &expected, &pinned) {
                diags.push(Diagnostic {
                    message: format!(
                        "`{callee}` needs `{}::{item}` to be {expected}, and this type \
                         parameter pins {pinned}",
                        bound.trait_name
                    ),
                    span,
                });
            }
        }
        return;
    }
    for (item, expected) in &bound.pins {
        let mut proj = args.clone();
        proj.push(target.clone());
        let actual = normalize(&Type::Named(assoc_type_name(&bound.trait_name, item), proj), env);
        if parse_assoc_type_name(match &actual {
            Type::Named(name, _) => name.as_str(),
            _ => "",
        })
        .is_some()
        {
            continue;
        }
        let expected = normalize(expected, env);
        if !same_type(env, &expected, &actual) {
            diags.push(Diagnostic {
                message: format!(
                    "`{callee}` needs `{}::{item}` to be {expected}, and {target} gives {actual}",
                    bound.trait_name
                ),
                span,
            });
        }
    }
}

/// Whether these two types are the same solved type.
///
/// Unification on a copy answers a different question: two flexible
/// variables meet by binding one to the other, and the copy records
/// nothing. A pin, or any other comparison, needs both sides solved.
fn same_type(env: &Env, expected: &Type, actual: &Type) -> bool {
    let expected = normalize(expected, env);
    let actual = normalize(actual, env);
    if has_open_var(&expected, env) || has_open_var(&actual, env) {
        return false;
    }
    let mut probe = env.uni.clone();
    probe.unify(&expected, &actual).is_ok()
}

fn types_agree(left: &[Type], right: &[Type]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| a == b)
}

/// `Into<i64>`, or the trait alone when it takes no arguments. Polarity
/// marks are dropped, the way an impl header writes the type.
fn trait_applied(trait_name: &str, args: &[Type]) -> String {
    if args.is_empty() {
        return trait_name.to_string();
    }
    let args = args
        .iter()
        .map(|ty| match ty {
            Type::Pos(base) | Type::Neg(base) => base.to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{trait_name}<{args}>")
}

fn has_open_var(ty: &Type, env: &Env) -> bool {
    match &env.uni.apply(ty) {
        Type::Var(v) => !env.uni.is_rigid(*v),
        Type::Named(_, args)
        | Type::Tensor(args)
        | Type::Par(args)
        | Type::With(args)
        | Type::Sum(args) => args.iter().any(|arg| has_open_var(arg, env)),
        Type::Dual(inner) | Type::Rowed(inner, _) | Type::Delayed(inner, _) => {
            has_open_var(inner, env)
        }
        _ => false,
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
    let self_ty = env.uni.fresh_var();
    let opened = open_trait(method, self_ty.clone(), env)?;
    let params: Vec<&slc_syntax::ast::Param> =
        sig.value_params.iter().chain(sig.continuation_params.iter()).collect();
    for (arg, param) in args.iter().zip(params.iter()) {
        let Some(expected) =
            param.ty.as_ref().and_then(|ty| resolve_subst(ty, &opened.subst, enums))
        else {
            check_expr(arg, enums, env, diags);
            continue;
        };
        if let Some(actual) = check_by_name(arg, enums, env, diags)
            && !fits_turning(env, &expected, &actual, arg)
        {
            let expected = env.uni.apply(&expected);
            diags.push(Diagnostic {
                message: format!("argument to `{method}` has type {actual}; expected {expected}"),
                span: arg.span,
            });
        }
    }
    // Discharge `Self: Trait` against what the arguments fixed it to. A
    // method whose `Self` appears only in what it consumes, or whose trait
    // parameter appears only in what it produces, learns the rest from the
    // context, which is checked after this call. Dispatch waits.
    let target = env.uni.apply(&self_ty);
    settle_method(method, &opened, &target, span, env, diags);
    if sig.is_command {
        return Some(Type::BOTTOM);
    }
    match &sig.return_type {
        Some(ty) => resolve_subst(ty, &opened.subst, enums).map(|t| {
            let t = match sig.polarity {
                slc_syntax::ast::FunctionPolarity::Negative => t.dual(),
                slc_syntax::ast::FunctionPolarity::Positive => t,
            };
            normalize(&t, env)
        }),
        None => Some(Type::ONE),
    }
}

/// `Self` and one fresh variable per trait parameter. The variables are what
/// the call's arguments and its expected type solve.
struct OpenedMethod {
    trait_name: String,
    names: Vec<String>,
    args: Vec<Type>,
    subst: HashMap<String, Type>,
}

fn open_trait(method: &str, self_ty: Type, env: &mut Env) -> Option<OpenedMethod> {
    let trait_name = env.traits.method_owner.get(method)?.clone();
    let names = env.traits.params_of(&trait_name).to_vec();
    let signs = env.traits.trait_param_signs.get(&trait_name).cloned().unwrap_or_default();
    let mut subst = HashMap::from([("Self".to_string(), self_ty.clone())]);
    let mut args = Vec::new();
    for name in &names {
        let var = env.uni.fresh_var();
        // `<+U>` says which way the parameter faces, even before the call's
        // context picks the type. A `let` of the call can then see a value.
        if let (Type::Var(index), Some((_, sign))) =
            (&var, signs.iter().find(|(param, _)| param == name))
        {
            env.rigid_signs.insert(*index, *sign);
        }
        subst.insert(name.clone(), var.clone());
        args.push(var);
    }
    // A bare associated name is the projection at this `Self`. Reducing it
    // waits until `Self` is a real type or a pinned parameter.
    if let Some(items) = env.traits.assocs.get(&trait_name) {
        for item in items {
            let mut proj = args.clone();
            proj.push(self_ty.clone());
            subst.insert(item.clone(), Type::Named(assoc_type_name(&trait_name, item), proj));
        }
    }
    Some(OpenedMethod { trait_name, names, args, subst })
}

/// Resolve the call now when `Self` and every trait argument are known.
/// Otherwise keep it until the declaration's type is finished: that is when
/// a result type, or a cut, has had its say.
fn settle_method(
    method: &str,
    opened: &OpenedMethod,
    self_ty: &Type,
    span: Span,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let self_ty = env.uni.apply(self_ty);
    let args: Vec<Type> = opened.args.iter().map(|arg| env.uni.apply(arg)).collect();
    let self_open = matches!(&self_ty, Type::Var(v) if !env.uni.is_rigid(*v));
    let args_open = args.iter().any(|arg| has_open_var(arg, env));
    if self_open || args_open {
        env.pending_methods.push(crate::env::PendingMethod {
            span,
            method: method.to_string(),
            trait_name: opened.trait_name.clone(),
            self_ty,
            trait_args: opened.args.clone(),
            trait_param_names: opened.names.clone(),
        });
        return;
    }
    resolve_method_dispatch(method, &opened.trait_name, &self_ty, &args, span, env, diags);
}

/// Would these meet, if we tried? The attempt runs on a copy of the
/// unification state, so a stage can ask before committing — a declared
/// callee whose parameters do not take what flows in may still read the
/// other way round, as `;` being commutative allows.
fn would_fit(env: &Env, expected: &Type, actual: &Type, expr: Option<&Expr>) -> bool {
    if actual == &Type::BOTTOM {
        return true;
    }
    let expected = normalize(expected, env);
    let actual = normalize(actual, env);
    let mut probe = env.uni.clone();
    if probe.unify(&expected, &actual).is_ok() {
        return true;
    }
    expr.is_some_and(|expr| numeric_literals_fit(expr, &env.uni.apply(&expected), &actual))
}

/// A tuple written in place, weighed component by component against a
/// callee's parameters: a numeric literal then takes the width or precision
/// its own slot requires, not the one the whole product happens to have. Like
/// `would_fit`, this commits nothing.
fn fits_piecewise(env: &Env, params: &[Type], actual: &Type, shape: &Expr) -> bool {
    let Expr::Pair(items) = shape else { return false };
    let components = tensor_spine(&env.uni.apply(actual));
    if items.len() != params.len() || items.len() != components.len() {
        return false;
    }
    let mut probe = env.clone();
    for ((item, actual), param) in items.iter().zip(&components).zip(params) {
        let before = probe.uni.clone();
        if probe.uni.unify(param, actual).is_ok() {
            continue;
        }
        probe.uni = before;
        if numeric_literals_fit(&item.kind, &probe.uni.apply(param), actual) {
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
///
/// The flag says the stage reads the other way round: a negative method,
/// `fn deliver(out: String) <- Self`, consumes the `Self` that flows in and
/// hands on what its continuation takes, as a negative `fn` stage does.
fn check_method_stage(
    method: &str,
    receiver: &Type,
    shape: &Expr,
    span: Span,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<(Type, bool)> {
    let sig = env.traits.method_sig(method)?.clone();
    let receiver = normalize(receiver, env);
    let receiver = &receiver;
    if sig.polarity == slc_syntax::ast::FunctionPolarity::Negative
        && !sig.is_command
        && let [out] = sig.value_params.as_slice()
    {
        let target = env.uni.apply(receiver);
        // A consumer flowing in is the continuation the method is handed: the
        // stage reads forward and produces the consumer of `Self`, which the
        // cut it stands in fixes, so dispatch waits for that.
        if target.is_negative() && !target.is_positive() {
            let self_ty = env.uni.fresh_var();
            let opened = open_trait(method, self_ty.clone(), env)?;
            let takes = out.ty.as_ref().and_then(|ty| resolve_subst(ty, &opened.subst, enums))?;
            let takes = env.uni.apply(&takes);
            let expects =
                if takes.is_positive() && !takes.is_negative() { takes.dual() } else { takes };
            if !fits(env, &expects, &target, shape) {
                diags.push(Diagnostic {
                    message: format!(
                        "`{method}` takes {expects} as its continuation, and what flows in has \
                         type {target}"
                    ),
                    span,
                });
            }
            settle_method(method, &opened, &self_ty, span, env, diags);
            let returns = sig
                .return_type
                .as_ref()
                .and_then(|ty| resolve_subst(ty, &opened.subst, enums))
                .unwrap_or(self_ty);
            return Some((normalize(&returns, env).dual(), false));
        }
        let opened = open_trait(method, target.clone(), env)?;
        settle_method(method, &opened, &target, span, env, diags);
        let takes = out.ty.as_ref().and_then(|ty| resolve_subst(ty, &opened.subst, enums))?;
        let takes = env.uni.apply(&takes);
        let flows_on =
            if takes.is_negative() && !takes.is_positive() { takes.dual() } else { takes };
        return Some((flows_on, true));
    }
    let self_var = env.uni.fresh_var();
    let opened = open_trait(method, self_var, env)?;
    let target = match sig.value_params.len() {
        // A method of several parameters takes them as one group: `Self`
        // is read off the components its parameters give that type.
        width if width >= 2 => receiver_of_group(
            method,
            &sig.value_params,
            receiver,
            shape,
            span,
            &opened.subst,
            enums,
            env,
            diags,
        )?,
        _ => {
            let target = env.uni.apply(receiver);
            let _ = env.uni.unify(opened.subst.get("Self").expect("Self"), &target);
            target
        }
    };
    settle_method(method, &opened, &target, span, env, diags);
    if sig.is_command {
        return Some((Type::BOTTOM, false));
    }
    match &sig.return_type {
        Some(ty) => resolve_subst(ty, &opened.subst, enums).map(|t| (normalize(&t, env), false)),
        None => Some((Type::ONE, false)),
    }
}

/// The `Self` of a method of several parameters, from the group flowing into
/// it: each component is checked against its parameter, the components that
/// are not integer literals first, so a literal takes its width from the
/// others — `<(1, x) | add` with `x: i32` is `i32`'s `add`.
#[allow(clippy::too_many_arguments)]
fn receiver_of_group(
    method: &str,
    params: &[slc_syntax::ast::Param],
    receiver: &Type,
    shape: &Expr,
    span: Span,
    subst: &HashMap<String, Type>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    let components = tensor_spine(&env.uni.apply(receiver));
    if components.len() != params.len() {
        diags.push(Diagnostic {
            message: format!(
                "`{method}` takes {} parameters as one group, and what flows in has type {}",
                params.len(),
                env.uni.apply(receiver)
            ),
            span,
        });
        return None;
    }
    let written: Vec<Option<&Expr>> = match shape {
        Expr::Pair(items) if items.len() == params.len() => {
            items.iter().map(|item| Some(&item.kind)).collect()
        }
        _ => vec![None; params.len()],
    };
    let self_ty = subst.get("Self").cloned().unwrap_or_else(|| env.uni.fresh_var());
    let mut order: Vec<usize> = (0..params.len()).collect();
    order.sort_by_key(|&i| written[i].is_some_and(is_integer_literal));
    // Only literals give `Self`: it is the default integer, as a bare
    // literal is.
    let all_literal = params.iter().zip(&written).all(|(param, written)| {
        !matches!(&param.ty, Some(TypeExpr::Base(name)) if name == "Self")
            || written.is_some_and(is_integer_literal)
    });
    if all_literal {
        let _ = env.uni.unify(&self_ty, &Type::Pos(Base::I64));
    }
    for index in order {
        let Some(expected) =
            params[index].ty.as_ref().and_then(|ty| resolve_subst(ty, subst, enums))
        else {
            continue;
        };
        let actual = &components[index];
        let fitted = match written[index] {
            Some(expr) => fits(env, &expected, actual, expr),
            None => fits(env, &expected, actual, &Expr::Pair(Vec::new())),
        };
        if !fitted {
            let expected = env.uni.apply(&expected);
            diags.push(Diagnostic {
                message: format!(
                    "`{method}` takes {expected} as parameter {}, and what flows in there has \
                     type {actual}",
                    index + 1
                ),
                span,
            });
        }
    }
    Some(env.uni.apply(&self_ty))
}

/// Record how a trait-method call dispatches, once `Self` is known: a
/// concrete receiver calls the impl directly, a bounded type parameter
/// projects the method from the enclosing function's dictionary.
fn resolve_method_dispatch(
    method: &str,
    trait_name: &str,
    target: &Type,
    trait_args: &[Type],
    span: Span,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let target = env.uni.apply(target);
    let trait_args: Vec<Type> = trait_args.iter().map(|arg| env.uni.apply(arg)).collect();
    discharge_bound(trait_name, &target, &trait_args, method, span, env, diags);
    let resolution = match &target {
        Type::Var(v) => env
            .bounds
            .iter()
            .find(|bound| {
                bound.var == *v
                    && bound.trait_name == trait_name
                    && types_agree(&bound.args, &trait_args)
            })
            .map(|bound| {
                let methods = env.traits.traits.get(trait_name);
                let count = methods.map(|m| m.len()).unwrap_or(1);
                let index =
                    methods.and_then(|m| m.iter().position(|tm| tm.name == method)).unwrap_or(0);
                slc_syntax::lower::MethodDispatch::Dict {
                    dict_var: slc_syntax::lower::dict_param_name_for(
                        trait_name,
                        &bound.arg_key,
                        &bound.type_param,
                    ),
                    index,
                    count,
                }
            }),
        _ => env.traits.select(trait_name, &target, &trait_args).ok().and_then(|matched| {
            env.traits
                .method_impls
                .get(method)
                .and_then(|impls| impls.get(&matched.key))
                .map(|mangled| slc_syntax::lower::MethodDispatch::Static(mangled.clone()))
        }),
    };
    // A static dispatch into a bounded impl — `impl<+T: Display> Display for
    // List<T>` — supplies one dictionary per impl bound, read off the
    // receiver and the impl's substitution.
    if let Some(slc_syntax::lower::MethodDispatch::Static(_)) = &resolution
        && let Ok(matched) = env.traits.select(trait_name, &target, &trait_args)
        && !matched.bounds.is_empty()
    {
        let mut dict_args = Vec::new();
        for bound in &matched.bounds {
            let Some(arg) = scrutinee_args(&target).get(bound.position).cloned() else { continue };
            let arg = env.uni.apply(&arg);
            let bound_args = {
                let Some(enums) = env.declarations else { continue };
                bound
                    .args
                    .iter()
                    .map(|ty| instantiate_bound_arg(ty, &matched.subst, enums))
                    .collect::<Option<Vec<_>>>()
                    .unwrap_or_default()
            };
            discharge_bound(&bound.trait_name, &arg, &bound_args, method, span, env, diags);
            if let Some(dict) = dict_for(&bound.trait_name, &arg, &bound_args, env) {
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

/// A bound argument written on an impl, with the impl's type parameters
/// replaced by the types this use gave them.
fn instantiate_bound_arg(
    ty: &TypeExpr,
    subst: &HashMap<String, Type>,
    enums: &Declarations,
) -> Option<Type> {
    resolve_subst(ty, subst, enums)
}

/// Resolve a method's written type. `subst` holds `Self` and the trait's
/// parameters; every other name goes through the ordinary declaration resolver.
fn resolve_subst(
    ty: &TypeExpr,
    subst: &HashMap<String, Type>,
    enums: &Declarations,
) -> Option<Type> {
    use slc_syntax::ast::TypeExpr as T;
    match ty {
        T::Base(name) => subst.get(name).cloned().or_else(|| enums.resolve(ty)),
        T::Positive(inner) => resolve_subst(&inner.kind, subst, enums),
        T::Negative(inner) if !inner.kind.is_bottom() => {
            Some(resolve_subst(&inner.kind, subst, enums)?.dual())
        }
        T::Dual(inner) => Some(resolve_subst(&inner.kind, subst, enums)?.dual()),
        T::Effectful(inner, row) => Some(Type::rowed(
            resolve_subst(&inner.kind, subst, enums)?,
            enums.resolve_row(row, |_| None, |ty| resolve_subst(ty, subst, enums))?,
        )),
        T::Apply(name, args) => {
            if let Some(found) = subst.get(name) {
                return Some(found.clone());
            }
            if enums.projection_arity(name).is_some() {
                let args = args
                    .iter()
                    .map(|arg| resolve_subst(&arg.kind, subst, enums))
                    .collect::<Option<Vec<_>>>()?;
                return enums.projected_type(name, args);
            }
            let args = args
                .iter()
                .map(|arg| resolve_subst(&arg.kind, subst, enums))
                .collect::<Option<Vec<_>>>()?;
            if enums.is_negative_decl(name) {
                Some(Type::Dual(Box::new(Type::Named(name.clone(), args))))
            } else if enums.declares(name) {
                Some(Type::Named(name.clone(), args))
            } else {
                None
            }
        }
        T::Tensor(items) => Some(Type::Tensor(subst_components(items, subst, enums)?)),
        T::Par(items) => Some(Type::Par(subst_components(items, subst, enums)?)),
        T::With(items) => Some(Type::With(subst_components(items, subst, enums)?)),
        T::Sum(items) => Some(Type::Sum(subst_components(items, subst, enums)?)),
        T::Fun(a, b) => Some(Type::arrow(
            resolve_subst(&a.kind, subst, enums)?,
            resolve_subst(&b.kind, subst, enums)?,
        )),
        _ => enums.resolve(ty),
    }
}

fn subst_components(
    items: &[Node<TypeExpr>],
    subst: &HashMap<String, Type>,
    enums: &Declarations,
) -> Option<Vec<Type>> {
    items.iter().map(|item| resolve_subst(&item.kind, subst, enums)).collect()
}

/// The type a `mu`'s arms name, when one of them does: a record pattern
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
    // A cut of two ending in the binder: `v | k>` makes `k` the consumer of
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
fn rigid_components(
    items: &[Node<TypeExpr>],
    rigid_vars: &HashMap<&str, Type>,
    enums: &Declarations,
) -> Option<Vec<Type>> {
    items.iter().map(|item| resolve_rigid(&item.kind, rigid_vars, enums)).collect()
}

/// A group of types as the one type a call passes: nothing is 1, one is
/// itself, and several are their tensor.
fn packed_group(types: impl IntoIterator<Item = Type>) -> Type {
    let mut types: Vec<Type> = types.into_iter().collect();
    match types.len() {
        1 => types.pop().expect("one type"),
        _ => Type::Tensor(types),
    }
}

/// A menu of exits as one type, the way a group of values packs: nothing is
/// 1, one is itself, and several are their `&`.
fn exit_row(types: impl IntoIterator<Item = Type>) -> Type {
    let mut types: Vec<Type> = types.into_iter().collect();
    match types.len() {
        0 => Type::ONE,
        1 => types.pop().expect("one type"),
        _ => Type::With(types),
    }
}

/// A declaration's row variables sit among its rigid type variables under
/// `..E`, a name no type has, as a row carried on the unit.
fn rigid_row_key(name: &str) -> String {
    format!("..{name}")
}

/// The rigid row variable a written `..E` stands for, in a body.
fn rigid_row(rigid_vars: &HashMap<&str, Type>, name: &str) -> Option<usize> {
    match rigid_vars.get(rigid_row_key(name).as_str()) {
        Some(Type::Rowed(_, carrier)) => carrier.tail,
        _ => None,
    }
}

/// What demanding a menu's item, or feeding a form, performs at this use: the
/// row it declares, with its row parameter's argument among `args` in the
/// parameter's place. `None` for a declaration that declares no row.
fn latent_row(
    enums: &Declarations,
    name: &str,
    args: &[Type],
    env: &mut Env,
) -> Option<slc_core::types::Row> {
    let concrete =
        enums.latent_rows.get(name).map(|row| row.map_types(|argument| argument.instantiate(args)));
    let Some(index) = enums.latent_row_param(name) else { return concrete };
    let argument = args.get(index).map(|arg| env.uni.apply(arg)).unwrap_or(Type::ONE);
    let (bare, given) = unrowed(argument);
    let given = match bare {
        // Not known yet: it stands for a row of its own.
        Type::Var(_) => {
            let fresh = slc_core::types::Row {
                effects: Default::default(),
                tail: Some(env.uni.fresh_row()),
            };
            let _ = env.uni.unify(&bare, &Type::Rowed(Box::new(Type::ONE), fresh.clone()));
            fresh
        }
        _ => given,
    };
    let mut latent = concrete.unwrap_or_default();
    latent.effects.extend(given.effects);
    latent.tail = latent.tail.or(given.tail);
    Some(latent)
}

/// Whether `ty` is the sum of an alternative `::i(v)` still waiting for its
/// context to name it: an unknown that would fit anything it meets.
fn is_pending_injection(env: &Env, ty: &Type) -> bool {
    let ty = env.uni.apply(ty);
    matches!(ty, Type::Var(_))
        && env.pending_injections.iter().any(|pending| env.uni.apply(&pending.sum) == ty)
}

/// A type without its row, and the row: what running a value of it performs.
fn unrowed(ty: Type) -> (Type, slc_core::types::Row) {
    match ty {
        Type::Rowed(inner, row) => (*inner, row),
        other => (other, slc_core::types::Row::default()),
    }
}

fn type_shape(ty: Type) -> Type {
    match ty {
        Type::Rowed(inner, _) | Type::Delayed(inner, _) => type_shape(*inner),
        other => other,
    }
}

fn force_type(ty: Type, env: &mut Env) -> Type {
    match env.uni.apply(&ty) {
        Type::Delayed(inner, row) => {
            env.perform(row);
            force_type(*inner, env)
        }
        Type::Rowed(inner, row) => Type::rowed(force_type(*inner, env), row),
        other => other,
    }
}

fn activation_type(ty: Type, env: &mut Env) -> Type {
    match env.uni.apply(&ty) {
        Type::Delayed(inner, row) | Type::Rowed(inner, row) => {
            env.perform(row);
            activation_type(*inner, env)
        }
        other => other,
    }
}

/// Where an argument meets the parameter it is passed to, for the row
/// constraints recorded there.
fn argument_origin(
    callee: &str,
    signature: &FunctionSignature,
    index: usize,
    argument: &Node<Expr>,
) -> crate::env::RowOrigin {
    crate::env::RowOrigin::Argument {
        callee: callee.to_string(),
        param: signature
            .param_names
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("argument {}", index + 1)),
        argument: match &argument.kind {
            Expr::Ident(name) => Some(name.clone()),
            _ => None,
        },
        span: argument.span,
    }
}

fn open_exit(ty: Type, env: &mut Env) -> Type {
    let bare = activation_type(ty, env);
    match bare {
        Type::With(items) => {
            Type::With(items.into_iter().map(|item| open_exit(item, env)).collect())
        }
        other => other,
    }
}

/// Check what a declaration's body performs against the row it writes, and
/// solve the row constraints it recorded from `from` on, keeping what does
/// not fit as the rows' diagnostics.
fn close_declaration_rows(
    env: &mut Env,
    body_row: usize,
    declared: slc_core::types::Row,
    from: usize,
    name: &str,
    span: Span,
) {
    let body = slc_core::types::Row { effects: Default::default(), tail: Some(body_row) };
    let origin = crate::env::RowOrigin::Declaration { name: name.to_string(), span };
    if let Some(index) = env.uni.row_constraints()[from..]
        .iter()
        .position(|constraint| constraint.sub == body && constraint.sup == declared)
    {
        env.row_origins.insert(from + index, origin);
    } else {
        env.constrain_row_for(body, declared, origin);
    }
    let constraints = env.uni.row_constraints()[from..].to_vec();
    for failure in env.uni.solve_rows(&constraints) {
        let (performs, addition) = match &failure.atom {
            slc_core::typing::RowAtom::Effect(effect) => {
                (format!("`{effect}`"), effect.to_string())
            }
            slc_core::typing::RowAtom::Rigid(var) => {
                let written = env.row_names.get(var).cloned().unwrap_or_else(|| format!("?{var}"));
                (format!("the row `..{written}`"), format!("..{written}"))
            }
        };
        let (message, at) = match (env.row_origins.get(&(from + failure.constraint)), &failure.atom)
        {
            (Some(crate::env::RowOrigin::Returned { name, ty, span }), _) => (
                format!(
                    "the value `{name}` hands back performs {performs}, but its return type {} \
                     does not allow it; declare the row on the returned type",
                    env.uni.apply(ty),
                ),
                *span,
            ),
            (
                Some(crate::env::RowOrigin::Declaration { name, span }),
                slc_core::typing::RowAtom::Effect(effect),
            ) => (
                format!(
                    "`{name}` performs `{effect}` but does not declare it; add `/ {{{effect}}}` \
                     to its type, or handle it"
                ),
                *span,
            ),
            (Some(crate::env::RowOrigin::Declaration { name, span }), _) => (
                format!(
                    "`{name}` performs {performs} of a parameter but does not declare it; add \
                     `{addition}` to its row"
                ),
                *span,
            ),
            (Some(crate::env::RowOrigin::Argument { callee, param, argument, span }), _) => {
                let allowed = &constraints[failure.constraint].sup;
                let arrow = if allowed.is_empty() {
                    " a pure arrow".to_string()
                } else {
                    format!(" row {allowed}")
                };
                let what = argument
                    .as_ref()
                    .map(|argument| format!("`{argument}`"))
                    .unwrap_or_else(|| "this argument".into());
                (
                    format!(
                        "`{callee}` takes `{param}` with{arrow} but {what} performs {performs}"
                    ),
                    *span,
                )
            }
            (Some(crate::env::RowOrigin::Latent { decl, span }), _) => (
                format!("this arm performs {performs}, which `{decl}` does not declare latent"),
                *span,
            ),
            (None, _) => (
                format!(
                    "`{name}` hands on a value that performs {performs} where the type it meets \
                     does not allow it"
                ),
                span,
            ),
        };
        let diagnostic = Diagnostic { message, span: at };
        if !env.row_diagnostics.contains(&diagnostic) {
            env.row_diagnostics.push(diagnostic);
        }
    }
}

fn resolve_rigid(
    ty: &TypeExpr,
    rigid_vars: &HashMap<&str, Type>,
    enums: &Declarations,
) -> Option<Type> {
    use slc_syntax::ast::TypeExpr as T;
    match ty {
        T::Base(name) => rigid_vars.get(name.as_str()).cloned().or_else(|| enums.resolve(ty)),
        T::Positive(inner) => resolve_rigid(&inner.kind, rigid_vars, enums),
        T::Negative(inner) if !inner.kind.is_bottom() => {
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
        // The effect row rides on the type it is written on; a row variable
        // is the declaration's own, rigid in its body.
        T::Effectful(inner, row) => Some(Type::rowed(
            resolve_rigid(&inner.kind, rigid_vars, enums)?,
            enums.resolve_row(
                row,
                |tail| rigid_row(rigid_vars, tail),
                |ty| resolve_rigid(ty, rigid_vars, enums),
            )?,
        )),
        T::Tensor(items) => Some(Type::Tensor(rigid_components(items, rigid_vars, enums)?)),
        T::Par(items) => Some(Type::Par(rigid_components(items, rigid_vars, enums)?)),
        T::With(items) => Some(Type::With(rigid_components(items, rigid_vars, enums)?)),
        T::Sum(items) => Some(Type::Sum(rigid_components(items, rigid_vars, enums)?)),
        // A row argument; its row variable is the declaration's own, rigid in
        // its body.
        T::Row(row) => {
            if row.tails.iter().any(|tail| rigid_row(rigid_vars, tail).is_none()) {
                return None;
            }
            Some(Type::rowed(
                Type::ONE,
                enums.resolve_row(
                    row,
                    |tail| rigid_row(rigid_vars, tail),
                    |ty| resolve_rigid(ty, rigid_vars, enums),
                )?,
            ))
        }
        T::Apply(name, args) => {
            if enums.projection_arity(name).is_some() {
                let args = args
                    .iter()
                    .map(|a| resolve_rigid(&a.kind, rigid_vars, enums))
                    .collect::<Option<Vec<_>>>()?;
                return enums.projected_type(name, args);
            }
            if !enums.args_match_kinds(name, args) {
                return None;
            }
            let args = args
                .iter()
                .map(|a| resolve_rigid(&a.kind, rigid_vars, enums))
                .collect::<Option<Vec<_>>>()?;
            let args = enums.complete_args(name, args)?;
            if name == "Delayed" {
                let inner = args.first()?;
                if inner.is_positive() && !inner.is_negative() {
                    return None;
                }
                Some(Type::delayed(inner.clone(), unrowed(args.get(1)?.clone()).1))
            } else if enums.is_negative_decl(name) {
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
fn unresolved_parameter_type(
    p: &slc_syntax::ast::Param,
    span: Span,
    enums: &Declarations,
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(ty) = &p.ty {
        unresolved_type(&format!("the type of parameter {}", p.describe()), ty, span, enums, diags);
    }
}

/// A written type that names nothing declared — refused, rather than left to
/// stand for any type at all. `what` says where it is written: "the return
/// type of `f`", "the type of field `x` of `D`".
fn unresolved_type(
    what: &str,
    ty: &TypeExpr,
    span: Span,
    enums: &Declarations,
    diags: &mut Vec<Diagnostic>,
) {
    // A row parameter given a type, or a type parameter a row, is the
    // likelier slip, and is named as such.
    let message = match enums.row_kind_mismatch(ty) {
        Some(mismatch) => format!("{what} {mismatch}"),
        None if matches!(ty, TypeExpr::Apply(name, _) if name == "Delayed") => format!(
            "{what} requires `(-> T / {{E}})` with a negative result type and a forcing row; \
             positive results use `lazy::Lazy<T, E>`"
        ),
        None if let Some(note) = projection_note(ty, enums) => format!("{what} {note}"),
        None => format!(
            "{what} names `{}`, which is not a declared type here; a library type is \
             `list::List`, or brought in with `cite`",
            type_display(ty)
        ),
    };
    diags.push(Diagnostic { message, span });
}

fn unresolved_return_type(
    owner: &str,
    ty: &TypeExpr,
    span: Span,
    enums: &Declarations,
    diags: &mut Vec<Diagnostic>,
) {
    unresolved_type(&format!("the return type of {owner}"), ty, span, enums, diags);
}

/// What a type declaration's fields, payloads and items name must exist.
/// Declarations are collected before anything is checked, and a type that
/// does not resolve there is only a placeholder, so it is refused here, each
/// against its declaration's own type parameters.
fn check_declared_types(p: &Program, enums: &Declarations, diags: &mut Vec<Diagnostic>) {
    fn scope(type_params: &[String]) -> HashMap<String, usize> {
        type_params.iter().enumerate().map(|(i, param)| (param.clone(), i)).collect()
    }
    for d in &p.decls {
        if let Decl::Fn { type_params, effects, .. }
        | Decl::Command { type_params, effects, .. }
        | Decl::Menu { type_params, effects, .. }
        | Decl::Form { type_params, effects, .. } = &d.kind
            && enums
                .resolve_row(effects, |_| None, |ty| enums.resolve_in(ty, &scope(type_params)))
                .is_none()
        {
            diags.push(Diagnostic {
                message: "invalid effect row: check effect names, argument arity and types, row parameters, and duplicate effects".into(),
                span: d.span,
            });
        }
        match &d.kind {
            Decl::Data { name, type_params, fields, .. }
            | Decl::Form { name, type_params, fields, .. } => {
                let params = scope(type_params);
                for (field, ty) in fields {
                    if enums.resolve_in(ty, &params).is_none() {
                        let what = format!("the type of field `{field}` of `{name}`");
                        unresolved_type(&what, ty, d.span, enums, diags);
                    }
                }
            }
            Decl::Menu { name, type_params, items, .. } => {
                let params = scope(type_params);
                for (item, ty) in items {
                    if enums.resolve_in(ty, &params).is_none() {
                        let what = format!("the type of item `{item}` of `{name}`");
                        unresolved_type(&what, ty, d.span, enums, diags);
                    }
                }
            }
            Decl::Enum { name, type_params, variants, .. } => {
                let params = scope(type_params);
                for (variant, payload) in variants {
                    for (index, ty) in payload.iter().enumerate() {
                        if enums.resolve_in(ty, &params).is_none() {
                            let what = format!("payload {index} of `{name}::{variant}`");
                            unresolved_type(&what, ty, d.span, enums, diags);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// A trait's method signatures outlive its declaration — elaboration keeps
/// them and drops the rest — so what their return types name is checked
/// here, with `Self` in scope.
/// An impl of a child trait requires each parent for the same type.
fn check_super_obligations(
    traits: &TraitInfo,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let mut names: Vec<&String> = traits.heads.keys().collect();
    names.sort();
    for trait_name in names {
        let parents = slc_syntax::traits::ancestors(traits, trait_name);
        if parents.is_empty() {
            continue;
        }
        let param_names = traits.trait_params.get(trait_name).map(Vec::as_slice).unwrap_or(&[]);
        for head in &traits.heads[trait_name] {
            let mut owned = HashMap::new();
            for param in &head.type_params {
                owned.insert(param.clone(), env.uni.fresh_rigid());
            }
            let rigid: HashMap<&str, Type> =
                owned.iter().map(|(name, ty)| (name.as_str(), ty.clone())).collect();
            let Some(self_ty) = resolve_rigid(&head.for_type, &rigid, enums) else {
                continue;
            };
            for parent in &parents {
                let subst: HashMap<&str, &slc_syntax::ast::TypeExpr> = param_names
                    .iter()
                    .zip(&head.trait_args)
                    .map(|(name, arg)| (name.as_str(), arg))
                    .collect();
                let args = parent
                    .args
                    .iter()
                    .map(|ty| {
                        resolve_rigid(&slc_syntax::traits::substitute(ty, &subst), &rigid, enums)
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(args) = args else { continue };
                if traits.select(&parent.trait_name, &self_ty, &args).is_err() {
                    diags.push(Diagnostic {
                        message: format!(
                            "`impl {trait_name} for {self_ty}` requires `{}`, which is not \
                             implemented for that type",
                            parent.trait_name
                        ),
                        span: head.span,
                    });
                }
            }
        }
    }
}

fn check_trait_signatures(
    traits: &TraitInfo,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let mut names: Vec<&String> = traits.traits.keys().collect();
    names.sort();
    for name in names {
        let span = traits.spans.get(name).copied().unwrap_or(Span { start: 0, end: 0 });
        let mut owned = HashMap::from([("Self".to_string(), env.uni.fresh_rigid())]);
        for param in traits.params_of(name) {
            owned.insert(param.clone(), env.uni.fresh_rigid());
        }
        // A bare associated name is the projection at this `Self`.
        let mut proj_args: Vec<Type> =
            traits.params_of(name).iter().filter_map(|param| owned.get(param).cloned()).collect();
        proj_args.push(owned["Self"].clone());
        for item in traits.assocs.get(name).map(Vec::as_slice).unwrap_or(&[]) {
            owned.insert(item.clone(), Type::Named(assoc_type_name(name, item), proj_args.clone()));
        }
        let rigid: HashMap<&str, Type> =
            owned.iter().map(|(param, ty)| (param.as_str(), ty.clone())).collect();
        for method in &traits.traits[name] {
            for param in method.value_params.iter().chain(method.continuation_params.iter()) {
                if let Some(written) = &param.ty
                    && resolve_rigid(written, &rigid, enums).is_none()
                {
                    unresolved_parameter_type(param, span, enums, diags);
                }
            }
            if let Some(written) = &method.return_type
                && resolve_rigid(written, &rigid, enums).is_none()
            {
                unresolved_return_type(
                    &format!("method `{}`", method.name),
                    written,
                    span,
                    enums,
                    diags,
                );
            }
        }
    }
}

/// Each impl's associated types name real types. Elaboration has already
/// substituted `Self` and the trait's parameters.
fn check_assoc_bindings(
    traits: &TraitInfo,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let mut names: Vec<&String> = traits.heads.keys().collect();
    names.sort();
    for trait_name in names {
        for head in &traits.heads[trait_name] {
            let mut owned = HashMap::new();
            for param in &head.type_params {
                owned.insert(param.clone(), env.uni.fresh_rigid());
            }
            let rigid: HashMap<&str, Type> =
                owned.iter().map(|(name, ty)| (name.as_str(), ty.clone())).collect();
            for (item, ty) in &head.assocs {
                if resolve_rigid(ty, &rigid, enums).is_none() {
                    unresolved_type(
                        &format!("`{item}` on `impl {trait_name}`"),
                        ty,
                        head.span,
                        enums,
                        diags,
                    );
                }
            }
        }
    }
}

/// Why a projection did not resolve, when the name is one and the argument
/// count is not the trait's parameters followed by the implementing type.
fn projection_note(ty: &TypeExpr, enums: &Declarations) -> Option<String> {
    let (name, written) = match ty {
        TypeExpr::Base(name) => (name.as_str(), 0),
        TypeExpr::Apply(name, args) => (name.as_str(), args.len()),
        _ => return None,
    };
    let arity = enums.projection_arity(name)?;
    if written == arity {
        return None;
    }
    Some(if written == 0 {
        format!("names `{name}` without the implementing type: write `{name}<…>`")
    } else {
        format!(
            "applies `{name}` to {written} type argument{}, and it takes {arity}: the trait's \
             arguments, then the implementing type",
            if written == 1 { "" } else { "s" }
        )
    })
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
        TypeExpr::Row(_) => "a row".to_string(),
        _ => "this type".into(),
    }
}

/// Resolve a type written in *body* position: a lambda's annotation, a
/// `let`'s, a `mu`'s or `mu`'s. The enclosing declaration's type
/// parameters come first, so `T` inside the body is the `T` the signature
/// bound — rigid, and carrying its bounds — rather than a fresh name.
fn resolve_in_body(ty: &TypeExpr, env: &Env, enums: &Declarations) -> Option<Type> {
    let resolved = if !env.rigid_vars.is_empty() {
        let rigid: HashMap<&str, Type> =
            env.rigid_vars.iter().map(|(name, ty)| (name.as_str(), ty.clone())).collect();
        resolve_rigid(ty, &rigid, enums)
    } else {
        None
    };
    let resolved = resolved.or_else(|| enums.resolve(ty))?;
    Some(normalize(&resolved, env))
}

/// What a declaration's body-scope replaced, to be restored after it: the
/// bounds in scope, and the type parameters' rigid variables.
type OuterScope = (Vec<crate::env::BoundInScope>, HashMap<String, Type>);

/// Put a declaration's bounds in scope for its body, and return the previous
/// set to restore afterward.
fn record_bounds(
    bounds: &[TraitBound],
    rigid_vars: &HashMap<&str, Type>,
    enums: &Declarations,
    env: &mut Env,
) -> OuterScope {
    let outer = env.bounds.clone();
    for bound in bounds {
        let Some(Type::Var(v)) = rigid_vars.get(bound.param.as_str()) else { continue };
        let Some(args) = bound
            .args
            .iter()
            .map(|ty| resolve_rigid(ty, rigid_vars, enums))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let pins = bound
            .pins
            .iter()
            .filter_map(|pin| {
                resolve_rigid(&pin.ty, rigid_vars, enums).map(|ty| (pin.name.clone(), ty))
            })
            .collect();
        env.bounds.push(crate::env::BoundInScope {
            var: *v,
            trait_name: bound.trait_name.clone(),
            type_param: bound.param.clone(),
            args,
            arg_key: slc_syntax::traits::rendered_args(&bound.args),
            pins,
        });
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
/// Resolve each `::i(v)` against the sum its context made it: count the
/// alternatives, check the position, and give the payload that
/// alternative's type.
fn resolve_pending_injections(env: &mut Env, diags: &mut Vec<Diagnostic>) {
    let mut waiting = std::mem::take(&mut env.pending_injections);
    // Resolving one alternative can say which sum another's payload belongs
    // to — an injection inside an injection — so go round while any resolves.
    loop {
        let before = waiting.len();
        let mut unresolved = Vec::new();
        for pending in waiting {
            let sum = env.uni.apply(&pending.sum);
            if let Type::Var(_) = sum {
                unresolved.push(pending);
                continue;
            }
            let index = pending.index;
            let Some(alternatives) = sum_alternatives(&sum) else {
                diags.push(Diagnostic {
                    message: format!(
                        "`::{index}` is an alternative of a sum, and it is used as {sum}"
                    ),
                    span: pending.span,
                });
                continue;
            };
            let Some(alternative) = alternatives.get(index) else {
                diags.push(Diagnostic {
                    message: format!(
                        "`::{index}` is out of range for {sum}, which has {} alternatives",
                        alternatives.len()
                    ),
                    span: pending.span,
                });
                continue;
            };
            let before = env.uni.clone();
            let fitted = env.uni.unify(alternative, &pending.payload).is_ok();
            if !fitted {
                env.uni = before;
            }
            if !fitted {
                let payload = env.uni.apply(&pending.payload);
                diags.push(Diagnostic {
                    message: format!(
                        "`::{index}` of {sum} carries {alternative}; this value has type {payload}"
                    ),
                    span: pending.span,
                });
            }
        }
        waiting = unresolved;
        if waiting.len() == before {
            break;
        }
    }
    // What is left belongs to no sum anything names: an alternative is built
    // by its position alone, so there is nothing to refuse.
}

/// Settle which way each component of a form value faces: a consumer takes
/// its part, and a value is taken by it.
fn resolve_pending_pars(env: &mut Env, diags: &mut Vec<Diagnostic>) {
    for pending in std::mem::take(&mut env.pending_pars) {
        let mut positives = Vec::new();
        for (index, component) in pending.components.iter().enumerate() {
            let ty = env.uni.apply(component);
            if let Type::Var(_) = ty {
                diags.push(Diagnostic {
                    message: format!(
                        "component {index} of this form value has no known type, so which way \
                         it faces is not known; give its type"
                    ),
                    span: pending.span,
                });
                break;
            }
            positives.push(ty.is_positive());
        }
        if positives.len() == pending.components.len() {
            env.dispatch.pars.insert(pending.span, positives);
        }
    }
}

/// A `mu` scrutinee that was a variable while checking. Once the declaration
/// is solved it must be positive data: a rigid `+T`, a variable that met
/// `<+T>`, or a ground positive type. A variable nothing signed is not a type
/// `mu` can consume.
fn resolve_pending_scrutinees(env: &mut Env, diags: &mut Vec<Diagnostic>) {
    for (span, ty) in std::mem::take(&mut env.pending_scrutinees) {
        let ty = normalize(&env.uni.apply(&ty), env);
        if type_polarity(&ty, env) == Some(ParamPolarity::Positive) {
            continue;
        }
        diags.push(Diagnostic {
            message: format!(
                "`mu` consumes data of positive type, and {ty} is not known to be one"
            ),
            span,
        });
    }
}

fn resolve_pending_dicts(env: &mut Env, enums: &Declarations, diags: &mut Vec<Diagnostic>) {
    resolve_pending_injections(env, diags);
    for (span, ty, chosen) in std::mem::take(&mut env.pending_computations) {
        let ty = env.uni.apply(&ty);
        let resolved = type_polarity(&ty, env);
        if !matches!(resolved, Some(ParamPolarity::Positive | ParamPolarity::Negative)) {
            diags.push(Diagnostic {
                message: format!(
                    "this by-name computation has type {ty}, whose evaluation polarity is not \
                     known; annotate its result, or evaluate it in `let+` before passing it"
                ),
                span,
            });
        } else if resolved != chosen {
            diags.push(Diagnostic {
                message: format!(
                    "the inferred evaluation polarity of {ty} changed while checking this \
                     computation; annotate its result to fix its demand boundary"
                ),
                span,
            });
        }
    }
    resolve_pending_pars(env, diags);
    resolve_pending_scrutinees(env, diags);
    for (span, consumer) in std::mem::take(&mut env.pending_consumers) {
        let consumer = type_shape(env.uni.apply(&consumer));
        let unsolved = matches!(&consumer, Type::Var(_))
            && type_polarity(&consumer, env) != Some(ParamPolarity::Negative);
        if matches!(&consumer, Type::Pos(_))
            || matches!(&consumer, Type::Named(name, _) if !enums.is_negative_decl(name))
            || unsolved
        {
            diags.push(Diagnostic {
                message: format!(
                    "the right of a cut must be a consumer; this expression has type {consumer}"
                ),
                span,
            });
        }
    }
    for pending in std::mem::take(&mut env.pending_methods) {
        let target = env.uni.apply(&pending.self_ty);
        let args: Vec<Type> = pending.trait_args.iter().map(|arg| env.uni.apply(arg)).collect();
        let open: Vec<&str> = args
            .iter()
            .zip(&pending.trait_param_names)
            .filter(|(ty, _)| has_open_var(ty, env))
            .map(|(_, name)| name.as_str())
            .collect();
        if !open.is_empty() {
            let names = open.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(", ");
            diags.push(Diagnostic {
                message: format!(
                    "`{}` needs a type for {names}; nothing fixes what it produces",
                    pending.method
                ),
                span: pending.span,
            });
            continue;
        }
        resolve_method_dispatch(
            &pending.method,
            &pending.trait_name,
            &target,
            &args,
            pending.span,
            env,
            diags,
        );
    }
    for pending in std::mem::take(&mut env.pending_dicts) {
        let mut dict_args = Vec::new();
        for bound in &pending.bounds {
            let target = env.uni.apply(&bound.var);
            let args: Vec<Type> = bound.args.iter().map(|arg| env.uni.apply(arg)).collect();
            discharge_bound(
                &bound.trait_name,
                &target,
                &args,
                &pending.callee,
                pending.span,
                env,
                diags,
            );
            check_pins(bound, &target, &args, &pending.callee, pending.span, env, diags);
            if let Some(dict) = dict_for(&bound.trait_name, &target, &args, env) {
                dict_args.push(dict);
            }
        }
        env.dispatch.calls.insert(pending.span, dict_args);
    }
    // A variable that meets parameters of both polarities has no type that
    // fits both, solved or not.
    let mut first_sign: HashMap<usize, (ParamPolarity, String, String)> = HashMap::new();
    let mut conflicted = std::collections::HashSet::new();
    for pending in &env.pending_signs {
        if pending.sign == ParamPolarity::Any {
            continue;
        }
        let Some((var, sign)) = signed_var(&env.uni.apply(&pending.ty), pending.sign) else {
            continue;
        };
        match first_sign.get(&var) {
            None => {
                first_sign.insert(var, (sign, pending.owner.clone(), pending.param.clone()));
            }
            Some((earlier, owner, param)) if *earlier != sign && conflicted.insert(var) => {
                let (earlier_mark, earlier_param) = (earlier.mark(), param.clone());
                diags.push(Diagnostic {
                    message: format!(
                        "one type meets `{owner}`'s `<{earlier_mark}{earlier_param}>` and `{}`'s \
                         `<{}{}>`, and no type is both positive and negative",
                        pending.owner,
                        pending.sign.mark(),
                        pending.param
                    ),
                    span: pending.span,
                });
            }
            Some(_) => {}
        }
    }
    let mut reported = std::collections::HashSet::new();
    for (span, name, ty) in std::mem::take(&mut env.pending_params) {
        let ty = env.uni.apply(&ty);
        // A lambda checked twice, as a probe and for real, is one parameter.
        if type_polarity(&ty, env).is_none() && reported.insert(span) {
            diags.push(Diagnostic {
                message: format!(
                    "the parameter `{name}` of this `fn` has type {ty}, whose polarity is not \
                     known: annotate it, `fn({name}: T)`"
                ),
                span,
            });
        }
    }
    for span in std::mem::take(&mut env.pending_names) {
        // What running it performs rides on its type, and it runs all the same.
        if env.expr_types.get(&span).is_some_and(|ty| type_shape(env.uni.apply(ty)) == Type::BOTTOM)
        {
            env.dispatch.runs.insert(span);
        }
    }
    for span in std::mem::take(&mut env.pending_by_name) {
        if let Some(ty) = env.expr_types.get(&span).map(|ty| env.uni.apply(ty))
            && type_polarity(&ty, env) == Some(ParamPolarity::Negative)
        {
            env.dispatch.delays.insert(span);
        }
    }
    for (span, ty) in std::mem::take(&mut env.pending_lets) {
        let ty = env.uni.apply(&ty);
        match type_polarity(&ty, env) {
            Some(ParamPolarity::Negative) => {
                env.dispatch.delays.insert(span);
            }
            Some(ParamPolarity::Positive) => {}
            None | Some(ParamPolarity::Any) => diags.push(Diagnostic {
                message: format!(
                    "this `let` binds a computation of type {ty}, whose polarity is not known, \
                     so whether it runs here or where it is used is not known: annotate it, or \
                     write `let+` to run it here or `let-` to delay it"
                ),
                span,
            }),
        }
    }
    for pending in std::mem::take(&mut env.pending_signs) {
        let ty = env.uni.apply(&pending.ty);
        if let Some(actual) = type_polarity(&ty, env)
            && pending.sign != ParamPolarity::Any
            && actual != pending.sign
        {
            let found = match actual {
                ParamPolarity::Positive => "positive",
                ParamPolarity::Negative => "negative",
                ParamPolarity::Any => "polarity-unrestricted",
            };
            diags.push(Diagnostic {
                message: format!(
                    "`{}` declares `<{}{}>`, and this use gives `{}` the {found} type {ty}",
                    pending.owner,
                    pending.sign.mark(),
                    pending.param,
                    pending.param
                ),
                span: pending.span,
            });
        }
    }
}

/// Record, for each signed type parameter of a called signature, the type
/// this call gives it.
fn record_signs(
    signature: &FunctionSignature,
    seen: &HashMap<usize, Type>,
    callee: &str,
    span: Span,
    env: &mut Env,
) {
    for (index, param, sign) in &signature.signs {
        if let Some(ty) = seen.get(index) {
            env.pending_signs.push(crate::env::PendingSign {
                span,
                owner: callee.to_string(),
                param: param.clone(),
                sign: *sign,
                ty: ty.clone(),
            });
        }
    }
}

/// The variable a type is, and the polarity `sign` gives it: the sign itself,
/// or flipped where the variable stands under a `dual`.
fn signed_var(ty: &Type, sign: ParamPolarity) -> Option<(usize, ParamPolarity)> {
    if sign == ParamPolarity::Any {
        return None;
    }
    match ty {
        Type::Var(var) => Some((*var, sign)),
        Type::Dual(inner) => signed_var(inner, sign.flipped()),
        Type::Rowed(inner, _) | Type::Delayed(inner, _) => signed_var(inner, sign),
        _ => None,
    }
}

/// The polarity an unsolved variable has: its parameter's mark for a rigid
/// one, and otherwise the sign of every generic parameter it has met. `None`
/// when nothing gives it one, or when two signs disagree.
fn var_sign(var: usize, env: &Env) -> Option<ParamPolarity> {
    if let Some(sign) = env.rigid_signs.get(&var) {
        return Some(*sign);
    }
    let mut found = None;
    for pending in &env.pending_signs {
        let Some((met, sign)) = signed_var(&env.uni.apply(&pending.ty), pending.sign) else {
            continue;
        };
        if met != var {
            continue;
        }
        match found {
            None => found = Some(sign),
            Some(earlier) if earlier != sign => return None,
            Some(_) => {}
        }
    }
    found
}

/// Remember the polarity each rigid variable's parameter declares.
fn record_rigid_signs(
    signs: &[(String, ParamPolarity)],
    rigid_vars: &HashMap<&str, Type>,
    env: &mut Env,
) {
    for (param, sign) in signs {
        if let Some(Type::Var(var)) = rigid_vars.get(param.as_str()) {
            env.rigid_signs.insert(*var, *sign);
        }
    }
}

/// The polarity a solved type has, when it has exactly one: a rigid
/// variable has its parameter's, and an unsolved variable has none yet.
fn type_polarity(ty: &Type, env: &Env) -> Option<ParamPolarity> {
    match ty {
        Type::Dual(inner) => type_polarity(inner, env).map(ParamPolarity::flipped),
        Type::Rowed(inner, _) | Type::Delayed(inner, _) => type_polarity(inner, env),
        Type::Var(var) => var_sign(*var, env),
        Type::Param(_) => None,
        ty if ty.is_positive() && !ty.is_negative() => Some(ParamPolarity::Positive),
        ty if ty.is_negative() && !ty.is_positive() => Some(ParamPolarity::Negative),
        _ => None,
    }
}

fn check_decl(d: &Node<Decl>, enums: &Declarations, env: &mut Env, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn {
            name,
            params,
            body,
            polarity,
            return_type,
            type_params,
            type_param_signs,
            bounds,
            effects,
            ..
        } => {
            env.push();
            // A type parameter is rigid inside the body: `T` is some type the
            // caller chose, not a licence to treat the value as any type. A
            // row variable, the parameter with no sign, is rigid there too.
            let row_keys = row_parameter_keys(type_params, type_param_signs, env);
            let mut rigid_vars: HashMap<&str, Type> =
                type_params.iter().map(|tp| (tp.as_str(), env.uni.fresh_rigid())).collect();
            for (key, carrier) in &row_keys {
                rigid_vars.insert(key.as_str(), carrier.clone());
            }
            let rows_from = env.uni.row_constraints().len();
            let body_row = env.uni.fresh_row();
            let outer_row = env.current_row.replace(body_row);
            let (outer_bounds, outer_rigid) = record_bounds(bounds, &rigid_vars, enums, env);
            record_rigid_signs(type_param_signs, &rigid_vars, env);
            let rigid = |ty: &TypeExpr| resolve_rigid(ty, &rigid_vars, enums);
            for p in params {
                match p.ty.as_ref().and_then(&rigid) {
                    Some(ty) => {
                        let ty = normalize(&ty, env);
                        screen_projections(&ty, d.span, env, diags);
                        bind_match_pattern(&p.pattern, &ty, enums, env);
                    }
                    None => unresolved_parameter_type(p, d.span, enums, diags),
                }
            }
            // A negative function produces the consumer of what follows its
            // `<-`, so that is what a `mu` in its body consumes.
            let outer = env.consumed.take();
            let declared = return_type.as_ref().and_then(&rigid).map(|ty| normalize(&ty, env));
            if let Some(written) = return_type
                && declared.is_none()
            {
                unresolved_return_type(&format!("`{name}`"), written, d.span, enums, diags);
            }
            if let Some(declared) = &declared {
                screen_projections(declared, d.span, env, diags);
            }
            env.consumed = (*polarity == slc_syntax::ast::FunctionPolarity::Negative)
                .then(|| declared.clone())
                .flatten();
            let body_type = check_expr(body, enums, env, diags);
            env.consumed = outer;
            // The body produces what the declaration promises: the return
            // type for `->`, its consumer for `<-`. A body that ends in a
            // cut produces nothing and promises nothing.
            let declared_row = enums
                .resolve_row(
                    effects,
                    |tail| rigid_row(&rigid_vars, tail),
                    |ty| resolve_rigid(ty, &rigid_vars, enums),
                )
                .unwrap_or_default();
            let promised = match polarity {
                slc_syntax::ast::FunctionPolarity::Positive => declared,
                // A negative function's row is performed on its watch: by
                // feeding the consumer it produces.
                slc_syntax::ast::FunctionPolarity::Negative => {
                    declared.map(|ty| Type::rowed(ty.dual(), declared_row.clone()))
                }
            };
            if let (Some(promised), Some(actual)) = (&promised, &body_type)
                && actual != &Type::BOTTOM
            {
                let (promised_bare, mut promised_row) = unrowed(env.uni.apply(promised));
                if promised_row.is_empty() {
                    promised_row = env.uni.latent_row(&promised_bare);
                }
                let (actual_bare, actual_row) = unrowed(env.uni.apply(actual));
                env.constrain_row_for(
                    actual_row,
                    promised_row,
                    crate::env::RowOrigin::Returned {
                        name: name.clone(),
                        ty: promised.clone(),
                        span: body.span,
                    },
                );
                if !fits_turning(env, &promised_bare, &actual_bare, tail_node(body)) {
                    diags.push(Diagnostic {
                        message: format!(
                            "the body of `{name}` has type {actual}; the declaration says \
                             {promised}"
                        ),
                        span: body.span,
                    });
                }
            }
            // Pending dispatch and injections are resolved once the promise has
            // been unified too: a return type may be all that says which sum
            // `::1(v)` belongs to.
            env.uni.constrain_row(
                slc_core::types::Row { effects: Default::default(), tail: Some(body_row) },
                declared_row.clone(),
            );
            env.uni.infer_row_arguments(rows_from);
            resolve_pending_dicts(env, enums, diags);
            env.current_row = outer_row;
            close_declaration_rows(env, body_row, declared_row, rows_from, name, d.span);
            env.bounds = outer_bounds;
            env.rigid_vars = outer_rigid;
            env.pop();
        }
        Decl::Command {
            name,
            value_params,
            continuation_params,
            body,
            return_type,
            type_params,
            type_param_signs,
            bounds,
            effects,
            ..
        } => {
            if let Some(return_type) = return_type
                && enums.resolve(return_type) != Some(Type::BOTTOM)
            {
                diags
                    .push(Diagnostic { message: "a `command` returns `(;)`".into(), span: d.span });
            }
            env.push();
            let row_keys = row_parameter_keys(type_params, type_param_signs, env);
            let mut rigid_vars: HashMap<&str, Type> =
                type_params.iter().map(|tp| (tp.as_str(), env.uni.fresh_rigid())).collect();
            for (key, carrier) in &row_keys {
                rigid_vars.insert(key.as_str(), carrier.clone());
            }
            let rows_from = env.uni.row_constraints().len();
            let body_row = env.uni.fresh_row();
            let outer_row = env.current_row.replace(body_row);
            let (outer_bounds, outer_rigid) = record_bounds(bounds, &rigid_vars, enums, env);
            record_rigid_signs(type_param_signs, &rigid_vars, env);
            for p in value_params.iter().chain(continuation_params.iter()) {
                match p.ty.as_ref().and_then(|ty| resolve_rigid(ty, &rigid_vars, enums)) {
                    Some(ty) => {
                        let ty = normalize(&ty, env);
                        screen_projections(&ty, d.span, env, diags);
                        bind_match_pattern(&p.pattern, &ty, enums, env);
                    }
                    None => unresolved_parameter_type(p, d.span, enums, diags),
                }
            }
            let body_type = check_expr(body, enums, env, diags);
            let declared = enums
                .resolve_row(
                    effects,
                    |tail| rigid_row(&rigid_vars, tail),
                    |ty| resolve_rigid(ty, &rigid_vars, enums),
                )
                .unwrap_or_default();
            env.uni.constrain_row(
                slc_core::types::Row { effects: Default::default(), tail: Some(body_row) },
                declared.clone(),
            );
            env.uni.infer_row_arguments(rows_from);
            resolve_pending_dicts(env, enums, diags);
            // A `command` consumes: every terminating path reaches a
            // continuation, so the body is `(;)`. Checked after unification,
            // so a flexible variable that nothing solved is not `(;)`.
            if let Some(actual) = body_type {
                let actual = if type_shape(env.uni.apply(&actual)) == Type::BOTTOM {
                    activation_type(actual, env)
                } else {
                    env.uni.apply(&actual)
                };
                if actual != Type::BOTTOM {
                    diags.push(Diagnostic {
                        message: format!(
                            "a `command` body must reach a continuation on every path (type `(;)`); \
                             this one has type {actual}"
                        ),
                        span: body.span,
                    });
                }
            }
            env.current_row = outer_row;
            // `main` is the root, and the runtime handles one effect: `IO`
            // is what may reach it, and everything else is handled before.
            if name == "main"
                && (declared
                    .effects
                    .iter()
                    .any(|effect| effect.name != crate::signatures::IO || !effect.args.is_empty())
                    || declared.tail.is_some())
            {
                env.row_diagnostics.push(Diagnostic {
                    message: "`main` is the root: the runtime handles `IO`, so its row is `{IO}` \
                              or empty and every other effect is handled before it"
                        .into(),
                    span: d.span,
                });
            }
            close_declaration_rows(env, body_row, declared, rows_from, name, d.span);
            env.bounds = outer_bounds;
            env.rigid_vars = outer_rigid;
            env.pop();
        }
        Decl::Const { name, ty, value, .. } => {
            if !is_constant_initializer(&value.kind, env) {
                diags.push(Diagnostic {
                    message: format!(
                        "def `{name}` initializer must be a literal or another definition"
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
                        "def `{name}` is annotated as {expected}; initializer has type {actual}"
                    ),
                    span: value.span,
                });
            }
        }
        Decl::Hand { name, clauses, ret, forward, effects, .. } => {
            let rows_from = env.uni.row_constraints().len();
            let node = Node {
                span: d.span,
                kind: Expr::Handler {
                    effects: vec![],
                    clauses: clauses.clone(),
                    ret: ret.clone(),
                    forward: *forward,
                },
            };
            let found = check_expr(&node, enums, env, diags);
            let Some(effects) = effects else { return };
            let Some(found) = found else { return };
            let declared = enums
                .resolve_row(
                    effects,
                    |_| None,
                    |ty| enums.resolve_in(ty, &std::collections::HashMap::new()),
                )
                .unwrap_or_default();
            let Type::Named(_, arguments) = env.uni.apply(&found) else { return };
            if arguments.len() != 4 {
                return;
            }
            let (_, residual) = unrowed(arguments[3].clone());
            let Some(var) = residual.tail else { return };
            close_declaration_rows(env, var, declared, rows_from, name, d.span);
        }
        // A signature has no body to check, but what it names must exist.
        Decl::Effect { operations, type_params, .. } => {
            let params =
                type_params.iter().enumerate().map(|(index, name)| (name.clone(), index)).collect();
            for op in operations {
                for parameter in &op.params {
                    if let Some(ty) = &parameter.ty
                        && enums.resolve_in(ty, &params).is_none()
                    {
                        unresolved_parameter_type(parameter, d.span, enums, diags);
                    }
                }
                if let Some(written) = &op.return_type
                    && enums.resolve_in(written, &params).is_none()
                {
                    unresolved_return_type(
                        &format!("operation `{}`", op.name),
                        written,
                        d.span,
                        enums,
                        diags,
                    );
                }
            }
        }
        _ => {}
    }
}

/// A declaration's row variables — its type parameters that declare no
/// sign — each as a fresh rigid row variable under its `..E` key, carried on
/// the unit. The names are kept for diagnostics.
fn row_parameter_keys(
    type_params: &[String],
    type_param_signs: &[(String, ParamPolarity)],
    env: &mut Env,
) -> Vec<(String, Type)> {
    type_params
        .iter()
        .filter(|param| type_param_signs.iter().all(|(signed, _)| signed != *param))
        .map(|param| {
            let var = env.uni.fresh_rigid_row();
            env.row_names.insert(var, param.clone());
            let carrier = slc_core::types::Row { effects: Default::default(), tail: Some(var) };
            (rigid_row_key(param), Type::Rowed(Box::new(Type::ONE), carrier))
        })
        .collect()
}

/// A computation in a by-name position, checked in a row of its own. A
/// negative one is delayed, so what it performs rides on its type; a
/// positive one is computed where it stands, so that is where it performs;
/// unresolved polarity uses the declaration's preliminary inference and
/// is validated against its final type before lowering.
fn check_by_name(
    item: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    if is_value_form(&item.kind, enums) {
        return check_expr(item, enums, env, diags);
    }
    let own = env.uni.fresh_row();
    let outer = env.current_row.replace(own);
    let actual = check_expr(item, enums, env, diags);
    env.current_row = outer;
    actual.map(|actual| carry_row(actual, own, item.span, env))
}

/// What a computation checked in row `own` produced: delayed when it is known
/// to be negative, carrying `own` on its type. Otherwise it performs here —
/// computed, when positive. Preliminary inference supplies a missing
/// polarity, and the final type must agree with that evaluation choice.
fn carry_row(actual: Type, own: usize, span: Span, env: &mut Env) -> Type {
    let runs = slc_core::types::Row { effects: Default::default(), tail: Some(own) };
    let applied = env.uni.apply(&actual);
    let known = type_polarity(&applied, env);
    let polarity = known.or_else(|| env.polarity_hints.get(&span).copied());
    if !matches!(known, Some(ParamPolarity::Positive | ParamPolarity::Negative)) {
        env.pending_computations.push((span, actual.clone(), polarity));
    }
    if polarity != Some(ParamPolarity::Negative) {
        env.perform(runs);
        return actual;
    }
    match applied {
        Type::Delayed(inner, carried) => {
            env.uni.constrain_row(carried, runs.clone());
            Type::delayed(*inner, runs)
        }
        other => Type::delayed(other, runs),
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
        | Expr::Ident(_)
        | Expr::Lambda { .. }
        | Expr::Select { .. }
        | Expr::Handler { .. } => true,
        Expr::Inject { value, .. } => is_value_form(&value.kind, enums),
        Expr::Pair(items) | Expr::Bundle(items) | Expr::Par(items) => {
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
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) => true,
        Expr::Ident(name) => env.constants.contains_key(name),
        _ => false,
    }
}

fn literal_type(e: &Expr) -> Option<Type> {
    Some(match e {
        Expr::Int(_) => Type::Pos(Base::I64),
        Expr::Float(_) => Type::Pos(Base::F64),
        Expr::Str(_) => Type::Pos(Base::Str),
        Expr::Char(_) => Type::Pos(Base::Char),
        _ => return None,
    })
}

fn pattern_type(pattern: &slc_syntax::ast::Pattern) -> Option<Type> {
    use slc_syntax::ast::Pattern;
    Some(match pattern {
        Pattern::Int(_) => Type::Pos(Base::I64),
        Pattern::Str(_) => Type::Pos(Base::Str),
        Pattern::Char(_) => Type::Pos(Base::Char),
        Pattern::Float(_) => Type::Pos(Base::F64),
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
        // A tuple's or a choice's components are its arguments.
        Type::Tensor(items) | Type::Sum(items) => items,
        Type::Dual(inner) | Type::Rowed(inner, _) | Type::Delayed(inner, _) => {
            scrutinee_args(inner)
        }
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
    trait_args: &[Type],
    env: &mut Env,
) -> Option<slc_syntax::lower::DictExpr> {
    let ty = normalize(&env.uni.apply(ty), env);
    let trait_args: Vec<Type> =
        trait_args.iter().map(|arg| normalize(&env.uni.apply(arg), env)).collect();
    if let Type::Var(v) = &ty {
        return env
            .bounds
            .iter()
            .find(|bound| {
                bound.var == *v
                    && bound.trait_name == bound_trait
                    && types_agree(&bound.args, &trait_args)
            })
            .map(|bound| slc_syntax::lower::DictExpr {
                name: slc_syntax::lower::dict_param_name_for(
                    bound_trait,
                    &bound.arg_key,
                    &bound.type_param,
                ),
                args: Vec::new(),
                methods: 1,
            });
    }
    let matched = env.traits.select(bound_trait, &ty, &trait_args).ok()?;
    let methods = env.traits.traits.get(bound_trait).map(|m| m.len()).unwrap_or(1);
    let mut args = Vec::new();
    if !matched.bounds.is_empty() {
        for bound in &matched.bounds {
            let inner_ty = env.uni.apply(scrutinee_args(&ty).get(bound.position)?);
            let inner_args = {
                let enums = env.declarations?;
                bound
                    .args
                    .iter()
                    .map(|ty| instantiate_bound_arg(ty, &matched.subst, enums))
                    .collect::<Option<Vec<_>>>()?
            };
            args.push(dict_for(&bound.trait_name, &inner_ty, &inner_args, env)?);
        }
    }
    Some(slc_syntax::lower::DictExpr {
        name: slc_syntax::lower::dict_global_name(bound_trait, &matched.key),
        args,
        methods,
    })
}

fn pending_bounds(
    signature: &FunctionSignature,
    seen: &HashMap<usize, Type>,
) -> Vec<crate::env::PendingBound> {
    signature
        .bounds
        .iter()
        .filter_map(|bound| {
            seen.get(&bound.param).map(|var| crate::env::PendingBound {
                trait_name: bound.trait_name.clone(),
                var: var.clone(),
                args: bound.args.clone(),
                pins: bound.pins.clone(),
            })
        })
        .collect()
}

/// Fresh unification variables for a declaration's type parameters, ready
/// to instantiate its stored field and payload types at one use.
fn fresh_args(enums: &Declarations, name: &str, env: &mut Env, span: Span) -> Vec<Type> {
    // A row parameter's argument is a row of its own, which the use says.
    let args: Vec<Type> = (0..enums.arity(name))
        .map(|index| {
            if enums.is_row_param(name, index) {
                let row = slc_core::types::Row {
                    effects: Default::default(),
                    tail: Some(env.uni.fresh_row()),
                };
                Type::Rowed(Box::new(Type::ONE), row)
            } else {
                env.uni.fresh_var()
            }
        })
        .collect();
    // What each is solved to must suit the polarity its parameter declares.
    for ((param, sign), ty) in enums.param_signs(name).iter().zip(&args) {
        if let Some(sign) = sign {
            env.pending_signs.push(crate::env::PendingSign {
                span,
                owner: name.to_string(),
                param: param.clone(),
                sign: *sign,
                ty: ty.clone(),
            });
        }
    }
    args
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
        && !numeric_pattern_fits(pattern, expected, &actual)
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
            if expected != &Type::Named(name.clone(), Vec::new()) {
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

/// Replace a projection whose implementing type is known. A concrete impl
/// gives its written type. A rigid parameter gives the type its bound pins,
/// or stays the projection — one type for that parameter, trait, and
/// arguments. An open variable stays until something solves it.
fn normalize(ty: &Type, env: &Env) -> Type {
    normalize_fuel(ty, env, 0)
}

fn normalize_fuel(ty: &Type, env: &Env, fuel: usize) -> Type {
    if fuel > 32 {
        return env.uni.apply(ty);
    }
    let ty = env.uni.apply(ty);
    match ty {
        Type::Named(name, args) => {
            let args: Vec<Type> = args.iter().map(|arg| normalize_fuel(arg, env, fuel)).collect();
            if let Some((trait_name, item)) = parse_assoc_type_name(&name)
                && let Some(reduced) = reduce_assoc(trait_name, item, &args, env, fuel)
            {
                return reduced;
            }
            Type::Named(name, args)
        }
        Type::Tensor(items) => {
            Type::Tensor(items.iter().map(|item| normalize_fuel(item, env, fuel)).collect())
        }
        Type::Par(items) => {
            Type::Par(items.iter().map(|item| normalize_fuel(item, env, fuel)).collect())
        }
        Type::With(items) => {
            Type::With(items.iter().map(|item| normalize_fuel(item, env, fuel)).collect())
        }
        Type::Sum(items) => {
            Type::Sum(items.iter().map(|item| normalize_fuel(item, env, fuel)).collect())
        }
        Type::Dual(inner) => normalize_fuel(&inner, env, fuel).dual(),
        Type::Rowed(inner, row) => Type::rowed(normalize_fuel(&inner, env, fuel), row),
        Type::Delayed(inner, row) => Type::delayed(normalize_fuel(&inner, env, fuel), row),
        other => other,
    }
}

/// The type a projection stands for, when an impl or a pin gives it one.
fn reduce_assoc(
    trait_name: &str,
    item: &str,
    args: &[Type],
    env: &Env,
    fuel: usize,
) -> Option<Type> {
    let (self_ty, trait_args) = args.split_last()?;
    let self_ty = env.uni.apply(self_ty);
    let trait_args: Vec<Type> = trait_args.iter().map(|arg| env.uni.apply(arg)).collect();
    match &self_ty {
        Type::Var(v) if env.uni.is_rigid(*v) => {
            let pinned = env
                .bounds
                .iter()
                .find(|bound| {
                    bound.var == *v
                        && bound.trait_name == trait_name
                        && types_agree(&bound.args, &trait_args)
                })
                .and_then(|bound| {
                    bound.pins.iter().find(|(name, _)| name == item).map(|(_, ty)| ty.clone())
                })?;
            Some(normalize_fuel(&pinned, env, fuel + 1))
        }
        Type::Var(_) => None,
        _ => {
            let matched = env.traits.select(trait_name, &self_ty, &trait_args).ok()?;
            let rhs = env.traits.heads.get(trait_name).and_then(|heads| {
                heads.iter().find(|head| head.key == matched.key).and_then(|head| {
                    head.assocs.iter().find(|(name, _)| name == item).map(|(_, ty)| ty.clone())
                })
            })?;
            let enums = env.declarations?;
            let ty = resolve_subst(&rhs, &matched.subst, enums)?;
            Some(normalize_fuel(&ty, env, fuel + 1))
        }
    }
}

/// A projection left in a written type must be fixed: a bound on a rigid
/// parameter, or an impl of a concrete type. An open variable waits.
fn screen_projections(ty: &Type, span: Span, env: &Env, diags: &mut Vec<Diagnostic>) {
    let ty = normalize(ty, env);
    let Type::Named(name, args) = &ty else {
        match &ty {
            Type::Tensor(items) | Type::Par(items) | Type::With(items) | Type::Sum(items) => {
                for item in items {
                    screen_projections(item, span, env, diags);
                }
            }
            Type::Dual(inner) | Type::Rowed(inner, _) | Type::Delayed(inner, _) => {
                screen_projections(inner, span, env, diags);
            }
            _ => {}
        }
        return;
    };
    let Some((trait_name, item)) = parse_assoc_type_name(name) else {
        for arg in args {
            screen_projections(arg, span, env, diags);
        }
        return;
    };
    let Some(self_ty) = args.last() else { return };
    let self_ty = env.uni.apply(self_ty);
    let trait_args: Vec<Type> =
        args[..args.len() - 1].iter().map(|arg| env.uni.apply(arg)).collect();
    match &self_ty {
        Type::Var(v) if env.uni.is_rigid(*v) => {
            let covered = env.bounds.iter().any(|bound| {
                bound.var == *v
                    && bound.trait_name == trait_name
                    && types_agree(&bound.args, &trait_args)
            });
            if !covered {
                let param = env
                    .rigid_vars
                    .iter()
                    .find_map(|(name, ty)| {
                        matches!(ty, Type::Var(found) if *found == *v).then_some(name.as_str())
                    })
                    .unwrap_or("this type");
                diags.push(Diagnostic {
                    message: format!(
                        "`{trait_name}::{item}` of `{param}` is not fixed; nothing implements \
                         `{trait_name}` for it"
                    ),
                    span,
                });
            }
        }
        Type::Var(_) => {}
        _ => {
            if let Err(message) = env.traits.select(trait_name, &self_ty, &trait_args) {
                diags.push(Diagnostic { message, span });
            }
        }
    }
}

/// Does a value written as `expr`, inferred as `actual`, fit a port that
/// requires `expected`?
///
/// A numeric literal takes the numeric type its port requires — `0 | exit`
/// sends an `i32` — and is `+i64`/`+f64` only when nothing constrains it.
/// Every other value must match its port exactly. `A -> B` and `B <- A`
/// are different types: a value written one way is not accepted at the other.
fn fits(env: &mut Env, expected: &Type, actual: &Type, expr: &Expr) -> bool {
    // A value that never arrives constrains nothing.
    if actual == &Type::BOTTOM {
        return true;
    }
    let expected = normalize(expected, env);
    let actual = normalize(actual, env);
    if env.uni.unify(&expected, &actual).is_ok() {
        return true;
    }
    // A numeric literal takes the width or precision its port requires.
    numeric_literals_fit(expr, &env.uni.apply(&expected), &actual)
}

/// A tuple written out is checked component by component, so a numeric
/// literal still takes the width of its own slot. The orientation of a
/// component is not turned to fit.
fn fits_turning(env: &mut Env, expected: &Type, actual: &Type, value: &Node<Expr>) -> bool {
    let before = env.uni.clone();
    if fits(env, expected, actual, &value.kind) {
        return true;
    }
    env.uni = before;
    if let Expr::Pair(items) = &value.kind
        && let (Type::Tensor(wanted), Type::Tensor(given)) =
            (env.uni.apply(expected), env.uni.apply(actual))
        && wanted.len() == items.len()
        && given.len() == items.len()
    {
        let probe = env.uni.clone();
        if items
            .iter()
            .zip(&wanted)
            .zip(&given)
            .all(|((item, want), give)| fits_turning(env, want, give, item))
        {
            return true;
        }
        env.uni = probe;
    }
    false
}

/// The type constructor two types share when they differ only by how a `;`
/// inside its arguments is spelled. That spelling is a different type.
fn turned_inside_constructor(expected: &Type, actual: &Type) -> Option<String> {
    fn canonical(ty: &Type) -> Type {
        let each = |items: &[Type]| items.iter().map(canonical).collect::<Vec<_>>();
        match ty {
            Type::Par(items) => {
                let mut items = each(items);
                if items.len() == 2 {
                    items.sort_by_key(|item| item.to_string());
                }
                Type::Par(items)
            }
            Type::Tensor(items) => Type::Tensor(each(items)),
            Type::With(items) => Type::With(each(items)),
            Type::Sum(items) => Type::Sum(each(items)),
            Type::Dual(inner) => Type::Dual(Box::new(canonical(inner))),
            Type::Rowed(inner, row) => Type::Rowed(Box::new(canonical(inner)), row.clone()),
            Type::Delayed(inner, row) => Type::delayed(canonical(inner), row.clone()),
            Type::Named(name, args) => Type::Named(name.clone(), each(args)),
            other => other.clone(),
        }
    }
    match (expected, actual) {
        (Type::Named(name, _), Type::Named(other, _))
            if name == other && expected != actual && canonical(expected) == canonical(actual) =>
        {
            Some(name.clone())
        }
        _ => None,
    }
}

/// `tail_expr`, as the node, so a mismatch can be reported on the expression
/// span of the expression that produces it.
fn tail_node(e: &Node<Expr>) -> &Node<Expr> {
    match &e.kind {
        Expr::Block(items) => items.last().map(tail_node).unwrap_or(e),
        Expr::Let { body: Some(body), .. } => tail_node(body),
        _ => e,
    }
}

fn is_integer_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Int(_))
}

fn is_integer_type(ty: &Type) -> bool {
    matches!(ty, Type::Pos(Base::I8 | Base::I32 | Base::I64 | Base::U8 | Base::U32 | Base::U64))
}

fn is_float_type(ty: &Type) -> bool {
    matches!(ty, Type::Pos(Base::F32 | Base::F64))
}

fn numeric_literals_fit(expr: &Expr, expected: &Type, actual: &Type) -> bool {
    match expr {
        Expr::Int(_) => is_integer_type(expected) && is_integer_type(actual),
        Expr::Float(_) => is_float_type(expected) && is_float_type(actual),
        _ => false,
    }
}

fn numeric_pattern_fits(
    pattern: &slc_syntax::ast::Pattern,
    expected: &Type,
    actual: &Type,
) -> bool {
    matches!(
        pattern,
        slc_syntax::ast::Pattern::Int(_)
            | slc_syntax::ast::Pattern::Float(_)
            | slc_syntax::ast::Pattern::Range { .. }
            | slc_syntax::ast::Pattern::Or(_)
    ) && ((is_integer_type(expected) && is_integer_type(actual))
        || (is_float_type(expected) && is_float_type(actual)))
}

/// A declaration's continuation row is positional and invariant: the
/// continuation supplied for a row position must have exactly the declared
/// type, and no position may be added, dropped, or reordered. A row is a
/// fixed calling interface, so one that differs in width or order is a
/// different interface, not a compatible one. Value arguments are checked
/// against their declared types the same way, including primitive calls.
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
            let actual = check_by_name(arg, enums, env, diags);
            if index < values {
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
            let row = (signature.params.len() > values)
                .then(|| exit_row(signature.params[values..].iter().cloned()));
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
        let actual = check_by_name(arg, enums, env, diags);
        let in_row = signature.continuations.get(index) == Some(&true);
        let Some(expected) = signature.params.get(index) else {
            continue;
        };
        let Some(actual) = actual else {
            continue;
        };
        // `Type::ONE` is this checker's "not determined" placeholder — an
        // unannotated `let` binding, for instance. A mismatch is only
        // reported for an argument whose type is actually known.
        let rows_from = env.uni.row_constraints().len();
        let fitted = fits(env, expected, &actual, &arg.kind);
        if !in_row {
            env.tag_rows_since(rows_from, argument_origin(name, signature, index, arg));
        }
        if !fitted {
            let expected = &env.uni.apply(expected);
            let message = if signature.builtin && !in_row {
                format!("argument to `{name}` has type {actual}; expected {expected}")
            } else if in_row {
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
    mode: slc_syntax::ast::LetMode,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Type {
    // A computation that may be delayed is checked in a row of its own, so
    // what it performs can ride on the name instead of happening here.
    let computes = mode != slc_syntax::ast::LetMode::Now && !is_value_form(&value.kind, enums);
    let own_row = computes.then(|| env.uni.fresh_row());
    let outer_row = env.current_row;
    if let Some(own) = own_row {
        env.current_row = Some(own);
    }
    let actual = check_expr(value, enums, env, diags);
    env.current_row = outer_row;
    let actual = actual.map(|actual| match mode {
        slc_syntax::ast::LetMode::Now => force_type(actual, env),
        _ => match own_row {
            Some(own) => carry_row(actual, own, value.span, env),
            None => actual,
        },
    });
    let annotation = ty.as_ref().and_then(|ty| resolve_in_body(ty, env, enums));
    if let Some(annotation) = &annotation {
        screen_projections(annotation, value.span, env, diags);
    }
    if let Some(written) = ty
        && annotation.is_none()
    {
        let what = match pattern.binder_name() {
            Some(name) => format!("the annotation of `let {name}`"),
            None => "the annotation of this `let`".into(),
        };
        unresolved_type(&what, written, value.span, enums, diags);
    }
    if let (Some(annotation), Some(actual)) = (&annotation, actual.clone())
        && !fits_turning(env, annotation, &actual, value)
    {
        let hint = turned_inside_constructor(&env.uni.apply(annotation), &env.uni.apply(&actual))
            .map(|name| {
                format!(
                    "; a `;` inside `{name}<…>` is spelled the other way round, and that \
                     spelling is a different type"
                )
            })
            .unwrap_or_default();
        diags.push(Diagnostic {
            message: match pattern.binder_name() {
                Some(name) => format!(
                    "`let {name}` is annotated as {annotation}; initializer has type {actual}{hint}"
                ),
                None => format!(
                    "this `let` is annotated as {annotation}; initializer has type {actual}{hint}"
                ),
            },
            span: value.span,
        });
    }
    if mode == slc_syntax::ast::LetMode::Delay {
        check_delayed_binding(pattern, annotation.as_ref().or(actual.as_ref()), value, env, diags);
    }
    // A binder the checker cannot type is a variable its uses will solve,
    // never a wildcard.
    let bound = annotation.or(actual).unwrap_or_else(|| env.uni.fresh_var());
    // A plain `let` of a computation follows its type: a negative one is
    // delayed, a positive one computed here. A value ran nothing either way.
    if mode == slc_syntax::ast::LetMode::Follow && !is_value_form(&value.kind, enums) {
        env.pending_lets.push((value.span, bound.clone()));
    }
    bound
}

/// `let-` holds a computation to run where it is demanded, so what it binds
/// is negative — a function, a consumer, a menu — and it binds a name: a
/// pattern takes a value apart, and a delayed computation is not one yet.
fn check_delayed_binding(
    pattern: &slc_syntax::ast::Pattern,
    ty: Option<&Type>,
    value: &Node<Expr>,
    env: &Env,
    diags: &mut Vec<Diagnostic>,
) {
    if pattern.binder_name().is_none() {
        diags.push(Diagnostic {
            message: "`let-` binds a name: a pattern takes a value apart, and a delayed \
                      computation is not a value until it runs"
                .into(),
            span: value.span,
        });
        return;
    }
    let ty = ty.map(|ty| env.uni.apply(ty));
    let message = match ty.as_ref().and_then(|ty| type_polarity(ty, env)) {
        Some(ParamPolarity::Negative) => return,
        Some(ParamPolarity::Positive) => format!(
            "`let-` delays a computation of negative type, and this one has the positive \
             type {}, which is computed where it is written: write `let` or `let+`",
            ty.expect("a polarity came from a type")
        ),
        None | Some(ParamPolarity::Any) => {
            "`let-` delays a computation of negative type, and this one's type is not \
                 known: annotate it"
                .into()
        }
    };
    diags.push(Diagnostic { message, span: value.span });
}

/// Bind a `of` pattern's variables with their declared types, silently
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
        // A request shape binds the continuation it carries. In a `of`
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
        // An alternative binds its payload, at its position in the sum.
        Pattern::Inject { index, pattern } => {
            if let Some(payload) = sum_alternatives(scrutinee).and_then(|a| a.get(*index).cloned())
            {
                bind_match_pattern(pattern, &payload, enums, env);
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
            && ty != Type::BOTTOM
            && ty != Type::ONE
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
        let k_ty = payload.first().cloned().unwrap_or(Type::ONE).instantiate(type_args);
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
/// the `mu` consumes.
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
        // A tensor: its components — none, for the unit.
        (Type::Tensor(..), Pattern::Tuple(_)) => flatten_tensor(consumed),
        // A menu of exits: its items, likewise.
        (Type::With(..), Pattern::Bundle(_)) => flatten_with(consumed),
        // An alternative of a sum: its payload. `check_alternatives` has
        // already said what is wrong with a position that is not there.
        (_, Pattern::Inject { index, .. }) => {
            match sum_alternatives(&env.uni.apply(consumed)).and_then(|a| a.get(*index).cloned()) {
                Some(payload) => vec![payload],
                None => return,
            }
        }
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
        Pattern::Inject { pattern, .. } => vec![pattern.as_ref()],
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

/// Bind one component of a `mu` arm. A nested product — a tuple or a
/// record — has one shape, so it may be taken apart in place; a sum inside a
/// component needs its own `of` in the arm.
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
            message: "a `mu` arm covers one shape: a sum or a value inside a component \
                      needs its own `of` in the arm"
                .into(),
            span,
        }),
    }
}

/// A tensor's components — none, for the unit. Anything else is one.
fn flatten_tensor(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Tensor(items) => items.clone(),
        other => vec![other.clone()],
    }
}

/// A sum's alternatives, when the type is a sum.
fn sum_alternatives(ty: &Type) -> Option<Vec<Type>> {
    match ty {
        Type::Sum(items) => Some(items.clone()),
        _ => None,
    }
}

/// Does a pattern hold an injection anywhere inside?
fn contains_injection(pattern: &slc_syntax::ast::Pattern) -> bool {
    use slc_syntax::ast::Pattern;
    match pattern {
        Pattern::Inject { .. } => true,
        Pattern::Or(items) | Pattern::Tuple(items) | Pattern::Bundle(items) => {
            items.iter().any(contains_injection)
        }
        Pattern::Enum { fields, .. } => fields.iter().any(contains_injection),
        Pattern::Data { fields, .. } => fields.iter().any(|(_, p)| contains_injection(p)),
        Pattern::Binding { pattern, .. } | Pattern::Dtor { arg: pattern, .. } => {
            contains_injection(pattern)
        }
        _ => false,
    }
}

/// The arms of a `mu` or `of` over a sum, by position. The sum is the
/// scrutinee's type — or, where that is still unknown, the sum of as many
/// alternatives as the arms name. A `mu` answers each position exactly
/// once and nothing else; a `of` covers every position, or has an arm
/// that matches anything. Returns the sum, its arity recorded for lowering.
fn check_alternatives(
    keyword: &str,
    consumed: &Type,
    arms: &[&slc_syntax::ast::Pattern],
    span: Span,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    use slc_syntax::ast::Pattern;
    let indices: Vec<usize> = arms
        .iter()
        .filter_map(|pattern| match pattern {
            Pattern::Inject { index, .. } => Some(*index),
            _ => None,
        })
        .collect();
    let mut ty = env.uni.apply(consumed);
    if let Type::Var(_) = ty {
        let arity = indices.iter().max().map_or(2, |most| (most + 1).max(2));
        let shape = Type::Sum((0..arity).map(|_| env.uni.fresh_var()).collect());
        let _ = env.uni.unify(&ty, &shape);
        ty = env.uni.apply(&shape);
    }
    let Some(alternatives) = sum_alternatives(&ty) else {
        diags.push(Diagnostic {
            message: format!(
                "`::{}` is an alternative of a sum, and this `{keyword}` is over {ty}",
                indices.first().copied().unwrap_or(0)
            ),
            span,
        });
        return None;
    };
    let arity = alternatives.len();
    let mut covered = vec![false; arity];
    let mut covers_everything = false;
    let mut well_formed = true;
    for pattern in arms.iter().copied() {
        match pattern {
            Pattern::Inject { index, pattern } => {
                if *index >= arity {
                    diags.push(Diagnostic {
                        message: format!(
                            "`::{index}` is out of range for {ty}, which has {arity} alternatives"
                        ),
                        span,
                    });
                    well_formed = false;
                    continue;
                }
                if contains_injection(pattern) {
                    diags.push(Diagnostic {
                        message: format!(
                            "take the payload of `::{index}` apart in the arm: an alternative's \
                             pattern holds no other alternative"
                        ),
                        span,
                    });
                    well_formed = false;
                }
                if keyword == "mu" {
                    if covered[*index] {
                        diags.push(Diagnostic {
                            message: format!("`mu` answers `::{index}` in more than one arm"),
                            span,
                        });
                    }
                    covered[*index] = true;
                } else if crate::exhaustive::is_irrefutable(pattern, enums) {
                    covered[*index] = true;
                }
            }
            _ if keyword == "mu" => {
                diags.push(Diagnostic {
                    message: "a `mu` over a sum covers its alternatives, `::0(x)` and \
                              `::1(y)`, and nothing else"
                        .into(),
                    span,
                });
                well_formed = false;
            }
            other => {
                if crate::exhaustive::is_irrefutable(other, enums) {
                    covers_everything = true;
                }
            }
        }
    }
    let missing: Vec<String> = covered
        .iter()
        .enumerate()
        .filter(|(_, covered)| !**covered)
        .map(|(index, _)| format!("`::{index}`"))
        .collect();
    if !covers_everything && !missing.is_empty() {
        diags.push(Diagnostic {
            message: format!(
                "non-exhaustive `{keyword}` over {ty}: missing {}",
                missing.join(", ")
            ),
            span,
        });
    }
    well_formed.then_some(ty)
}

fn flatten_with(ty: &Type) -> Vec<Type> {
    match ty {
        Type::With(items) => items.clone(),
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

fn elaboration_span(origin: Span, env: &mut Env) -> Span {
    let end = usize::MAX - env.elaboration_origins.len() * 2;
    let span = Span { start: end - 1, end };
    let origin = env.elaboration_origins.get(&origin).copied().unwrap_or(origin);
    env.elaboration_origins.insert(span, origin);
    span
}

fn is_command(stage: &Node<Expr>, env: &Env) -> bool {
    matches!(&stage.kind, Expr::Ident(name)
        if env.lookup(name).is_none()
            && !env.traits.is_method(name)
            && env.functions.get(name).is_some_and(|signature|
                signature.continuations.iter().any(|continuation| *continuation)
                    && signature.result.as_ref() == Some(&Type::BOTTOM)))
}

fn group_flow(expression: &Node<Expr>, env: &mut Env) -> Option<Node<Expr>> {
    let Expr::Flow { stages, from_value, into_consumer } = &expression.kind else {
        return None;
    };
    if !from_value {
        let argument_span = elaboration_span(expression.span, env);
        let param = format!("$flow_{}", argument_span.start);
        let mut body_stages = vec![Node { span: argument_span, kind: Expr::Ident(param.clone()) }];
        body_stages.extend_from_slice(stages);
        let body = Node {
            span: elaboration_span(expression.span, env),
            kind: Expr::Flow {
                stages: body_stages,
                from_value: true,
                into_consumer: *into_consumer,
            },
        };
        return Some(Node {
            span: expression.span,
            kind: Expr::Lambda { param, param_type: None, return_type: None, body: Box::new(body) },
        });
    }
    let command = stages.len() >= 3 && is_command(&stages[stages.len() - 2], env);
    let prefix = stages.len().checked_sub(if command { 2 } else { 1 })?;
    if prefix < 2 {
        return None;
    }
    let mut grouped = vec![Node {
        span: elaboration_span(expression.span, env),
        kind: Expr::Flow {
            stages: stages[..prefix].to_vec(),
            from_value: true,
            into_consumer: false,
        },
    }];
    grouped.extend_from_slice(&stages[prefix..]);
    Some(Node {
        span: expression.span,
        kind: Expr::Flow { stages: grouped, from_value: true, into_consumer: *into_consumer },
    })
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
    let open_head = match &e.kind {
        Expr::Flow { stages, from_value: false, .. } => stages.first(),
        _ => None,
    };
    let grouped = group_flow(e, env);
    if let Some(grouped) = &grouped {
        env.dispatch.elaborated.insert(e.span, grouped.clone());
    }
    let e = grouped.as_ref().unwrap_or(e);
    let arms_row = matches!(e.kind, Expr::Select { .. }).then(|| env.uni.fresh_row());
    let outer_row = env.current_row;
    if let Some(arms_row) = arms_row {
        env.current_row = Some(arms_row);
    }
    let found = check_expr_unapplied(e, enums, env, diags);
    if let Some(head) = open_head
        && let Some(ty) = env.expr_types.get(&head.span)
    {
        let ty = type_shape(env.uni.apply(ty));
        if !matches!(&ty, Type::Par(parts) if !parts.is_empty()) && !matches!(ty, Type::Var(_)) {
            diags.retain(|diagnostic| {
                diagnostic.span != head.span || !diagnostic.message.starts_with("a step composes")
            });
            diags.push(Diagnostic {
                message: format!(
                    "a chain without `<` begins with a function, and this has type \
                     {ty}; send it as a value: `<… | …`"
                ),
                span: head.span,
            });
        }
    }
    env.current_row = outer_row;
    let found = match arms_row {
        Some(arms_row) => found.map(|ty| {
            Type::rowed(
                ty,
                slc_core::types::Row { effects: Default::default(), tail: Some(arms_row) },
            )
        }),
        None => found,
    };
    let found = found.map(|ty| env.uni.apply(&ty));
    if let Some(ty) = &found {
        env.expr_types.insert(e.span, ty.clone());
    }
    found
}

/// Note the computations among `items`, which stand in by-name positions:
/// each is delayed if it turns out negative. A value ran nothing, so it is
/// passed as it is.
fn note_by_name(items: &[Node<Expr>], enums: &Declarations, env: &mut Env) {
    for item in items {
        if !is_value_form(&item.kind, enums) {
            env.pending_by_name.push(item.span);
        }
    }
}

/// The components a positional projection can reach: a product's, and a
/// menu of exits' — taking an exit is projecting an item of a `&`, which the
/// runtime holds as the same tuple. Anything else is a single component.
fn tensor_spine(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Tensor(items) | Type::With(items) if items.len() >= 2 => items.clone(),
        other => vec![other.clone()],
    }
}

fn yielding_exit_menu(
    expected: &Type,
    actual: Type,
    span: Span,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<(Type, Type, usize)> {
    let declared = flatten_with(&env.uni.apply(expected));
    let forcing_var = env.uni.fresh_row();
    let outer_row = env.current_row.replace(forcing_var);
    let actual = activation_type(actual, env);
    env.current_row = outer_row;
    let forcing = slc_core::types::Row { effects: Default::default(), tail: Some(forcing_var) };
    env.perform(forcing.clone());
    let callbacks = flatten_with(&actual);
    if callbacks.len() != declared.len() {
        diags.push(Diagnostic {
            message: format!(
                "this command offers {} exits, but the yielding bundle contains {} functions",
                declared.len(),
                callbacks.len()
            ),
            span,
        });
        return None;
    }
    let answer = env.uni.fresh_var();
    let mut consumers = Vec::new();
    for (index, callback) in callbacks.iter().enumerate() {
        let callback_var = env.uni.fresh_row();
        let outer_row = env.current_row.replace(callback_var);
        let callback = activation_type(callback.clone(), env);
        env.current_row = outer_row;
        let row = slc_core::types::Row { effects: Default::default(), tail: Some(callback_var) };
        let Type::Par(parts) = &callback else {
            diags.push(Diagnostic {
                message: format!(
                    "exit {} is a consumer or non-function ({callback}); a command yields a \
                     value only when every exit is a returning function",
                    index + 1
                ),
                span,
            });
            return None;
        };
        if parts.len() != 2 || parts[1] == Type::BOTTOM {
            diags.push(Diagnostic {
                message: format!(
                    "exit {} is a consumer, not a returning function; a command yields a \
                     value only when every exit does",
                    index + 1
                ),
                span,
            });
            return None;
        }
        if env.uni.unify(&answer, &parts[1]).is_err() {
            diags.push(Diagnostic {
                message: format!(
                    "exit {} has result type {}, but the other returning exits produce {}",
                    index + 1,
                    env.uni.apply(&parts[1]),
                    env.uni.apply(&answer)
                ),
                span,
            });
            return None;
        }
        let consumer_row =
            slc_core::types::Row { effects: Default::default(), tail: Some(env.uni.fresh_row()) };
        env.uni.constrain_row(row, consumer_row.clone());
        env.uni.constrain_row(forcing.clone(), consumer_row.clone());
        env.perform(consumer_row.clone());
        consumers.push(Type::rowed(parts[0].clone(), consumer_row));
    }
    Some((exit_row(consumers), env.uni.apply(&answer), callbacks.len()))
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
            env.pending_names.push(e.span);
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
            if enums.hands.contains(name) {
                diags.push(Diagnostic {
                    message: format!("`{name}` is a hand; install it with `do expr {name}`"),
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
                let (signature, seen) = instantiate(signature, &mut env.uni);
                record_signs(&signature, &seen, name, e.span, env);
                if let Some(result) = signature.result {
                    // Naming a returning function never invokes it. A
                    // parameterless consumer transformer instead denotes
                    // the consumer that lowering installs directly.
                    let ty = if signature.nullary_value {
                        Type::rowed(result, signature.row)
                    } else {
                        Type::rowed(
                            Type::arrow(packed_group(signature.params), result),
                            signature.row,
                        )
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
                                 its module's path, or brought in with `cite`"
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
            let type_args = fresh_args(enums, &declaration, env, e.span);
            Some(Type::Named(declaration, type_args))
        }
        Expr::Lambda { param, param_type, return_type, body } => {
            if let Some(written) = return_type
                && resolve_in_body(written, env, enums).is_none()
            {
                unresolved_return_type("this lambda", written, e.span, enums, diags);
            }
            env.push();
            let param_ty = param_type
                .as_ref()
                .and_then(|ty| resolve_in_body(ty, env, enums))
                .or_else(|| infer_param_type(param, body, enums, env))
                .unwrap_or_else(|| env.uni.fresh_var());
            if param_type.is_none() {
                env.pending_params.push((e.span, param.clone(), param_ty.clone()));
            }
            env.define(param, param_ty.clone());
            // A lambda performs nothing where it is written: its body's row is
            // the row of the function it is.
            let body_row = env.uni.fresh_row();
            let outer_row = env.current_row.replace(body_row);
            let result = check_expr(body, enums, env, diags);
            env.current_row = outer_row;
            env.pop();
            // A lambda is a function value, and its result is what the body
            // produces. A body that ends in a cut produces nothing, and
            // `(A -> (;))` is `-A`, so such a lambda simply *is* a consumer.
            let result = result.unwrap_or_else(|| env.uni.fresh_var());
            let runs = slc_core::types::Row { effects: Default::default(), tail: Some(body_row) };
            Some(Type::Rowed(Box::new(Type::arrow(param_ty, result)), runs))
        }
        Expr::Call { callee, args } => {
            note_by_name(args, enums, env);
            // A variant applied to its payload is a value, not a call.
            if let Expr::Ident(name) = &callee.kind
                && env.lookup(name).is_none()
                && let Some((declaration, payload)) = enums.variant(name)
            {
                let declaration = declaration.clone();
                let payload = payload.clone();
                // A generic declaration's payload types carry its
                // parameters; each construction instantiates them fresh.
                let type_args = fresh_args(enums, &declaration, env, e.span);
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
                    if let Some(actual) = check_by_name(arg, enums, env, diags)
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
            // an atomic consumer is certainly not a function — `A -> B` is
            // `(dual(A) ; B)`, so a function is negative too, and a `;` may be
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
                && (matches!(type_shape(ty.clone()), Type::Neg(_) | Type::Dual(_))
                    || matches!(type_shape(ty.clone()), Type::Par(ref parts) if parts.is_empty()))
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` is a consumer of type {ty}, not a function; send it a value \
                         with a cut: `<value | {name}>`"
                    ),
                    span: e.span,
                });
                for arg in args {
                    check_expr(arg, enums, env, diags);
                }
                return Some(Type::BOTTOM);
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
                env.perform(signature.row.clone());
                // A function is applied by flowing into it: `x | f`, not
                // `f(x)`. A command takes both its groups from the chain —
                // `(xs, i) | nth | (found & missing)>` — and a constructor
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
                record_signs(&signature, &seen, name, e.span, env);
                if !signature.bounds.is_empty() {
                    env.pending_dicts.push(crate::env::PendingDicts {
                        span: e.span,
                        callee: name.clone(),
                        bounds: pending_bounds(&signature, &seen),
                    });
                }
                return signature.result.map(|ty| normalize(&ty, env));
            }
            // A local callee: a closure, or a binder whose type its uses
            // decide. `A -> B` is `(dual(A) ; B)`, so application peels a `;`, and
            // an unknown callee becomes one.
            // Calling it performs what its type says running it does.
            let callee_ty = callee_ty.map(|ty| activation_type(ty, env));
            match callee_ty {
                Some(Type::Par(parts)) if parts.len() == 2 => {
                    let (argument_dual, result) = (parts[0].clone(), parts[1].clone());
                    // The arguments pack into one product, as they do for a
                    // named callee, so the whole group meets the one type
                    // the function takes.
                    let actuals: Vec<Option<Type>> =
                        args.iter().map(|arg| check_by_name(arg, enums, env, diags)).collect();
                    let packed = actuals
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .map(packed_group)
                        .unwrap_or(Type::ONE);
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
        Expr::Let { pattern, ty, value, body, mode } => {
            let binding_ty = check_let_binding(pattern, ty, value, *mode, enums, env, diags);
            env.push();
            bind_let_pattern(pattern, binding_ty, value, enums, env);
            let result = body.as_ref().and_then(|body| check_expr(body, enums, env, diags));
            env.pop();
            result
        }
        Expr::Match { scrutinee, arms } => {
            let scrutinee_ty = check_expr(scrutinee, enums, env, diags);
            // A sum's alternatives, by position.
            let scrutinee_ty = match scrutinee_ty {
                Some(ty)
                    if arms.iter().any(|arm| {
                        matches!(arm.pattern, slc_syntax::ast::Pattern::Inject { .. })
                    }) =>
                {
                    let rows: Vec<&slc_syntax::ast::Pattern> =
                        arms.iter().map(|arm| &arm.pattern).collect();
                    check_alternatives("of", &ty, &rows, e.span, enums, env, diags)
                }
                other => other,
            };
            if let Some(scrutinee_ty) = &scrutinee_ty {
                for arm in arms {
                    check_pattern(&arm.pattern, scrutinee_ty, enums, arm.body.span, diags);
                }
            }
            let mut joined = Some(Type::BOTTOM);
            for arm in arms {
                env.push();
                // Bind the arm's pattern variables with their declared types,
                // so the body sees `h: i64` for `Cons(h, _)`.
                if let Some(scrutinee_ty) = &scrutinee_ty {
                    bind_match_pattern(&arm.pattern, scrutinee_ty, enums, env);
                }
                let arm_ty = check_expr(&arm.body, enums, env, diags);
                env.pop();
                // The match has the type its arms agree on. An arm that ends
                // in a cut never returns, so it constrains nothing; an arm of
                // unknown type leaves the match's unknown too.
                joined = match (joined, arm_ty) {
                    (joined, Some(arm_ty)) if arm_ty == Type::BOTTOM => joined,
                    (Some(joined), Some(arm_ty)) if joined == Type::BOTTOM => Some(arm_ty),
                    (Some(joined), Some(arm_ty)) => {
                        if env.uni.unify(&joined, &arm_ty).is_err() {
                            let joined = env.uni.apply(&joined);
                            let arm_ty = env.uni.apply(&arm_ty);
                            diags.push(Diagnostic {
                                message: format!(
                                    "`of` arms have incompatible types {joined} and {arm_ty}"
                                ),
                                span: arm.body.span,
                            });
                        }
                        Some(joined)
                    }
                    _ => None,
                };
            }
            if arms.is_empty() { None } else { joined }
        }
        Expr::Data { name, fields } => {
            let type_args = fresh_args(enums, name, env, e.span);
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
                note_by_name(std::slice::from_ref(value), enums, env);
                let actual = check_by_name(value, enums, env, diags);
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
            Some(Type::Named(name.clone(), type_args))
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
                // `(&)`: the empty menu itself, which answers no demand.
                Some(TypeExpr::With(items)) if items.is_empty() => {
                    if !arms.is_empty() {
                        diags.push(Diagnostic {
                            message: "`(&)` is the empty menu, which answers no demand".into(),
                            span: e.span,
                        });
                    }
                    return Some(Type::TOP);
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
            let type_args = written_args.unwrap_or_else(|| fresh_args(enums, &menu, env, e.span));
            let rows: Vec<(&slc_syntax::ast::Pattern, &Node<Expr>)> =
                arms.iter().map(|arm| (&arm.pattern, &arm.command)).collect();
            // The arms run per demand. Over a menu that declares a latent row
            // they must fit inside it; otherwise what they perform is the row
            // of the menu value.
            let arms_row = env.uni.fresh_row();
            let outer_row = env.current_row.replace(arms_row);
            check_comatch_arms(&menu, &type_args, rows, enums, env, diags);
            env.current_row = outer_row;
            let runs = slc_core::types::Row { effects: Default::default(), tail: Some(arms_row) };
            let latent = latent_row(enums, &menu, &type_args, env);
            let menu_ty = Type::Dual(Box::new(Type::Named(menu.clone(), type_args)));
            match latent {
                Some(latent) => {
                    env.constrain_row_for(
                        runs,
                        latent,
                        crate::env::RowOrigin::Latent { decl: menu, span: e.span },
                    );
                    Some(menu_ty)
                }
                None => Some(Type::Rowed(Box::new(menu_ty), runs)),
            }
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
                            message: "`mu` needs a declared type or an explicit connective".into(),
                            span: ty.span,
                        });
                        return None;
                    }
                },
                // Left out: an arm's pattern may name the type, and inside a
                // negative `fn` the declaration already said it.
                None => named_by_arms(arms, enums).or_else(|| env.consumed.clone()).or_else(|| {
                    // Arms that name positions consume some sum; which one,
                    // the positions and the cut will say.
                    arms.iter()
                        .any(|arm| matches!(arm.pattern, slc_syntax::ast::Pattern::Inject { .. }))
                        .then(|| env.uni.fresh_var())
                }),
            };
            let Some(resolved) = resolved else {
                diags.push(Diagnostic {
                    message: "no arm names a type, so write what this `mu` consumes".into(),
                    span: e.span,
                });
                return None;
            };
            // A menu belongs to `mu`: `mu` answers data, and a menu
            // answers demands.
            if let Type::Dual(inner) = &resolved
                && let Type::Named(menu, _) = inner.as_ref()
                && enums.is_menu(menu)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`mu` answers data, and `{menu}` is a menu, which answers demands: \
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
                && let Type::Named(form, form_args) = inner.as_ref()
                && enums.is_form(form)
            {
                let demand = Type::Named(form.clone(), form_args.clone());
                // A form with a latent row runs its arms when fed: they fit
                // inside that row, and perform nothing where it is written.
                let latent = latent_row(enums, form, form_args, env);
                let arms_row = latent.as_ref().map(|_| env.uni.fresh_row());
                let outer_row = env.current_row;
                if let (Some(latent), Some(arms_row)) = (latent, arms_row) {
                    env.current_row = Some(arms_row);
                    let runs =
                        slc_core::types::Row { effects: Default::default(), tail: Some(arms_row) };
                    env.constrain_row_for(
                        runs,
                        latent,
                        crate::env::RowOrigin::Latent { decl: form.clone(), span: e.span },
                    );
                }
                for arm in arms {
                    env.push();
                    bind_select_arm(&demand, &arm.pattern, enums, env, e.span, diags);
                    let command = check_expr(&arm.command, enums, env, diags);
                    if let Some(command) = command
                        && command != Type::BOTTOM
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "a `mu` arm is a command; this one has type {command}"
                            ),
                            span: arm.command.span,
                        });
                    }
                    env.pop();
                }
                env.current_row = outer_row;
                return Some(resolved);
            }
            // A variable is settled at the end of the declaration: a rigid
            // `+T` is positive data, and a variable nothing has signed is not.
            if has_open_var(&resolved, env) || matches!(resolved, Type::Var(_)) {
                env.pending_scrutinees.push((e.span, resolved.clone()));
            } else if resolved.is_negative() && !resolved.is_positive() {
                diags.push(Diagnostic {
                    message: format!(
                        "`mu` consumes data, and {resolved} is a consumer; `mu` \
                         builds the consumer of a positive type"
                    ),
                    span: e.span,
                });
                return None;
            }
            let consumed = if arms
                .iter()
                .any(|arm| matches!(arm.pattern, slc_syntax::ast::Pattern::Inject { .. }))
            {
                let rows: Vec<&slc_syntax::ast::Pattern> =
                    arms.iter().map(|arm| &arm.pattern).collect();
                check_alternatives("mu", &resolved, &rows, e.span, enums, env, diags)?
            } else {
                resolved.clone()
            };
            for arm in arms {
                env.push();
                bind_select_arm(&consumed, &arm.pattern, enums, env, e.span, diags);
                let command = check_expr(&arm.command, enums, env, diags);
                if let Some(command) = command
                    && command != Type::BOTTOM
                {
                    diags.push(Diagnostic {
                        message: format!("a `mu` arm is a command; this one has type {command}"),
                        span: arm.command.span,
                    });
                }
                env.pop();
            }
            Some(resolved.dual())
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
            let type_args = fresh_args(enums, &menu, env, e.span);
            let expected = payload.first().cloned().unwrap_or(Type::ONE).instantiate(&type_args);
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
            // Demanding an item runs the menu: it performs the menu's row.
            let base_ty = force_type(base_ty, env);
            let (base_ty, base_row) = unrowed(base_ty);
            env.perform(base_row);
            match key {
                // `base.i` — the i-th component along the tensor spine.
                slc_syntax::ast::ProjKey::Index(i) => {
                    let components = tensor_spine(&base_ty);
                    match components.get(*i) {
                        Some(ty) => {
                            env.dispatch.projections.insert(
                                e.span,
                                slc_syntax::lower::Projection {
                                    index: *i,
                                    arity: components.len(),
                                    record: None,
                                },
                            );
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
                        && let Type::Named(menu, menu_args) = inner.as_ref()
                        && enums.is_menu(menu)
                    {
                        let label = format!("{menu}::{name}");
                        if let Some(latent) = latent_row(enums, menu, menu_args, env) {
                            env.perform(latent);
                        }
                        let args = scrutinee_args(&base_ty).to_vec();
                        return match enums.variant(&label) {
                            Some((_, payload)) => {
                                env.dispatch.demands.insert(e.span, label);
                                Some(
                                    payload
                                        .first()
                                        .cloned()
                                        .unwrap_or(Type::ONE)
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
                    // from `(-A ; -B)` there is no `-A` to be had, the way
                    // `(A, B)` yields its `A`. Say so, rather than leaving it
                    // at "not a record".
                    if let Type::Dual(inner) = &base_ty
                        && let Type::Named(form, _) = inner.as_ref()
                        && enums.is_form(form)
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "`{form}` is a form, and a form takes every field at once: send \
                                 it one, `<{form} {{ … }} | …>`. A single field cannot be taken \
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
                                env.dispatch.projections.insert(
                                    e.span,
                                    slc_syntax::lower::Projection {
                                        index,
                                        arity: fields.len(),
                                        record: Some(record_name.clone()),
                                    },
                                );
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
        Expr::Handler { effects, clauses, ret, forward } => {
            let inferred = clauses
                .iter()
                .filter_map(|clause| enums.op_effects.get(&clause.op).cloned())
                .collect::<std::collections::BTreeSet<_>>();
            let declared = if effects.is_empty() {
                inferred.clone()
            } else {
                effects.iter().cloned().collect()
            };
            for effect in &declared {
                if !enums.effects.contains(effect) {
                    diags.push(Diagnostic {
                        message: format!("`{effect}` is not a declared effect"),
                        span: e.span,
                    });
                }
                if !forward
                    && !inferred.contains(effect)
                    && enums.op_effects.values().any(|owner| owner == effect)
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "this handler has no clauses for its declared effect `{effect}`"
                        ),
                        span: e.span,
                    });
                }
            }
            for effect in inferred.difference(&declared) {
                diags.push(Diagnostic {
                    message: format!(
                        "this handler answers `{effect}`, which is absent from its effect list"
                    ),
                    span: e.span,
                });
            }
            let declared = declared
                .into_iter()
                .map(|name| {
                    let args = fresh_args(enums, &name, env, e.span);
                    slc_core::types::Effect { name, args }
                })
                .collect::<std::collections::BTreeSet<_>>();
            let input = env.uni.fresh_var();
            let residual = env.uni.fresh_row();
            let body_row = slc_core::types::Row { effects: declared.clone(), tail: Some(residual) };
            let thunk_type = Type::rowed(Type::arrow(Type::ONE, input.clone()), body_row);
            let fake_body = Node {
                span: elaboration_span(e.span, env),
                kind: Expr::Flow {
                    stages: vec![
                        Node { span: elaboration_span(e.span, env), kind: Expr::Pair(Vec::new()) },
                        Node {
                            span: elaboration_span(e.span, env),
                            kind: Expr::Ident("$handler_computation".into()),
                        },
                    ],
                    from_value: true,
                    into_consumer: false,
                },
            };
            let definition = Node {
                span: elaboration_span(e.span, env),
                kind: Expr::Handle {
                    body: Box::new(fake_body),
                    clauses: clauses.clone(),
                    ret: ret.clone(),
                    forward: *forward,
                },
            };
            env.push();
            env.define("$handler_computation", thunk_type);
            let outer = env.current_row.replace(residual);
            // The clause and the capability row start as two argument vectors.
            // Equate them before the handler value is generalized: a `let`
            // copies the capability's variables, and a copy made first can be
            // instantiated at a type the clause has already ruled out.
            let rows_from = env.uni.row_constraints().len();
            let answer = check_expr(&definition, enums, env, diags);
            env.uni.infer_row_arguments(rows_from);
            env.current_row = outer;
            env.pop();
            let declared = declared
                .into_iter()
                .map(|effect| slc_core::types::Effect {
                    name: effect.name,
                    args: effect.args.iter().map(|arg| env.uni.apply(arg)).collect(),
                })
                .collect();
            Some(Type::Named(
                "Handler".into(),
                vec![
                    env.uni.apply(&input),
                    answer?,
                    Type::rowed(
                        Type::ONE,
                        slc_core::types::Row {
                            effects: if *forward { Default::default() } else { declared },
                            tail: None,
                        },
                    ),
                    Type::rowed(
                        Type::ONE,
                        slc_core::types::Row { effects: Default::default(), tail: Some(residual) },
                    ),
                ],
            ))
        }
        Expr::WithHandler { handler, body } => {
            let handler_type = check_expr(handler, enums, env, diags)?;
            let Type::Named(name, arguments) = env.uni.apply(&handler_type) else {
                diags.push(Diagnostic {
                    message: format!("`do` needs a handler; this has type {handler_type}"),
                    span: handler.span,
                });
                return None;
            };
            if name != "Handler" || arguments.len() != 4 {
                diags.push(Diagnostic {
                    message: "`do` needs an `(A hn B / {E} / {F})` value".into(),
                    span: handler.span,
                });
                return None;
            }
            let handled = unrowed(arguments[2].clone()).1;
            let residual = unrowed(arguments[3].clone()).1;
            let body_row = env.uni.fresh_row();
            let outer = env.current_row.replace(body_row);
            let actual = check_expr(body, enums, env, diags);
            if let Some(actual) = &actual
                && type_shape(env.uni.apply(actual)) == Type::BOTTOM
            {
                activation_type(actual.clone(), env);
            }
            env.current_row = outer;
            if let Some(actual) = actual
                && !fits_turning(env, &arguments[0], &actual, body)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "this handler takes {}; its body produces {actual}",
                        arguments[0]
                    ),
                    span: body.span,
                });
            }
            let mut allowed = residual.clone();
            allowed.effects.extend(handled.effects);
            env.uni.constrain_row(
                slc_core::types::Row { effects: Default::default(), tail: Some(body_row) },
                allowed,
            );
            env.perform(residual);
            Some(arguments[1].clone())
        }
        Expr::Handle { body, clauses, ret, forward } => {
            let handler_row = env.uni.fresh_row();
            let ambient_row = env.current_row.replace(handler_row);
            let mut answered =
                std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
            for clause in clauses {
                if let Some(effect) = enums.op_effects.get(&clause.op)
                    && !answered.entry(effect.clone()).or_default().insert(clause.op.clone())
                {
                    diags.push(Diagnostic {
                        message: format!("duplicate `op` clause for `{}`", clause.op),
                        span: clause.body.span,
                    });
                }
            }
            if !forward {
                for (effect, operations) in &answered {
                    let mut missing = enums
                        .op_effects
                        .iter()
                        .filter(|(operation, owner)| {
                            *owner == effect && !operations.contains(*operation)
                        })
                        .map(|(operation, _)| operation.as_str())
                        .collect::<Vec<_>>();
                    missing.sort_unstable();
                    if !missing.is_empty() {
                        diags.push(Diagnostic {
                            message: format!(
                                "`{effect}` has {} operations, but this handler does not answer {}; answer every operation, or add `_ => forward`",
                                operations.len() + missing.len(),
                                missing.iter().map(|operation| format!("`{operation}`")).collect::<Vec<_>>().join(", "),
                            ),
                            span: e.span,
                        });
                    }
                }
            }
            let handled = answered
                .keys()
                .map(|name| slc_core::types::Effect {
                    name: name.clone(),
                    args: fresh_args(enums, name, env, e.span),
                })
                .collect::<std::collections::BTreeSet<_>>();
            let mut signatures = HashMap::new();
            for clause in clauses {
                if let Some(signature) = env.functions.get(&clause.op) {
                    let signature = instantiate(signature, &mut env.uni).0;
                    if let Some(effect) = signature.row.effects.iter().next()
                        && let Some(expected) =
                            handled.iter().find(|expected| expected.name == effect.name)
                    {
                        env.uni.constrain_row(
                            signature.row.clone(),
                            slc_core::types::Row { effects: [expected.clone()].into(), tail: None },
                        );
                    }
                    signatures.insert(clause.op.clone(), signature);
                }
            }
            // The body runs under the handler; its normal value feeds the
            // return clause, whose body is the handle's type.
            // The body performs into a row of its own. The handler answers
            // the effects of the operations its clauses name, and the rest
            // reaches the body around it; the clauses run below the prompt.
            let body_row = env.uni.fresh_row();
            let outer_row = env.current_row.replace(body_row);
            let body_ty =
                check_expr(body, enums, env, diags).unwrap_or_else(|| env.uni.fresh_var());
            // A body of type `(;)` stands as a command: a program handed in
            // as an exit runs here, under the handler, so what it performs is
            // the body's to perform, and the handle is a command too.
            let body_ty = if type_shape(env.uni.apply(&body_ty)) == Type::BOTTOM {
                activation_type(body_ty, env)
            } else {
                body_ty
            };
            env.current_row = outer_row;
            if let Some(outer) = outer_row {
                env.uni.constrain_row(
                    slc_core::types::Row { effects: Default::default(), tail: Some(body_row) },
                    slc_core::types::Row { effects: handled, tail: Some(outer) },
                );
                if *forward {
                    env.uni.constrain_row(
                        slc_core::types::Row { effects: Default::default(), tail: Some(body_row) },
                        slc_core::types::Row { effects: Default::default(), tail: Some(outer) },
                    );
                }
            }
            let answer_ty = match ret {
                Some((binder, return_body)) => {
                    env.push();
                    env.define(binder, body_ty.clone());
                    let answer = check_expr(return_body, enums, env, diags)
                        .unwrap_or_else(|| env.uni.fresh_var());
                    env.pop();
                    answer
                }
                None => body_ty,
            };
            for clause in clauses {
                // A clause for no operation would answer nothing, and a
                // misspelt one would leave its effect unhandled with no word
                // of why.
                if !enums.op_effects.contains_key(&clause.op) {
                    diags.push(Diagnostic {
                        message: format!("`{}` is not an operation of any effect", clause.op),
                        span: e.span,
                    });
                }
                env.push();
                // A clause takes what its operation is performed with, and
                // `resume` takes what the operation answers.
                let signature = signatures.remove(&clause.op);
                if let Some(signature) = &signature
                    && signature.params.len() != clause.params.len()
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "`{}` takes {} parameter{}, and this clause binds {}",
                            clause.op,
                            signature.params.len(),
                            if signature.params.len() == 1 { "" } else { "s" },
                            clause.params.len(),
                        ),
                        span: clause.body.span,
                    });
                }
                for (index, p) in clause.params.iter().enumerate() {
                    let v = signature
                        .as_ref()
                        .and_then(|signature| signature.params.get(index).cloned())
                        .unwrap_or_else(|| env.uni.fresh_var());
                    env.define(p, v);
                }
                let resume_in = signature
                    .as_ref()
                    .and_then(|signature| signature.result.clone())
                    .unwrap_or_else(|| env.uni.fresh_var());
                env.define(
                    &clause.resume,
                    Type::rowed(
                        Type::Par(vec![resume_in.dual(), answer_ty.clone()]),
                        slc_core::types::Row {
                            effects: Default::default(),
                            tail: Some(handler_row),
                        },
                    ),
                );
                if let Some(clause_ty) = check_expr(&clause.body, enums, env, diags)
                    && type_shape(env.uni.apply(&clause_ty)) != Type::BOTTOM
                    && !fits_turning(env, &answer_ty, &clause_ty, &clause.body)
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "handler clause `{}` has type {}; the handler answers {}",
                            clause.op,
                            env.uni.apply(&clause_ty),
                            env.uni.apply(&answer_ty),
                        ),
                        span: clause.body.span,
                    });
                }
                env.pop();
            }
            env.current_row = ambient_row;
            env.perform(slc_core::types::Row {
                effects: Default::default(),
                tail: Some(handler_row),
            });
            Some(answer_ty)
        }
        // `::i(v)` — one alternative of a sum. Which sum is the context's to
        // say; once the declaration's unification is done, the payload meets
        // the alternative at its position, if the sum is known by then.
        Expr::Inject { index, value } => {
            note_by_name(std::slice::from_ref(value), enums, env);
            let payload = check_by_name(value, enums, env, diags)?;
            let sum = env.uni.fresh_var();
            env.pending_injections.push(crate::env::PendingInjection {
                span: e.span,
                value_span: value.span,
                index: *index,
                payload,
                sum: sum.clone(),
            });
            Some(sum)
        }
        // `(k1 ; k2)` — a form value, one continuation per component. Which
        // way each of its cuts faces is its component's polarity, settled once
        // the declaration's unification is done.
        Expr::Par(items) => {
            let types = items
                .iter()
                .map(|item| check_expr(item, enums, env, diags))
                .collect::<Option<Vec<_>>>()?;
            let activation = slc_core::types::Row {
                effects: Default::default(),
                tail: Some(env.uni.fresh_row()),
            };
            let types = types
                .into_iter()
                .map(|ty| {
                    let (bare, row) = unrowed(env.uni.apply(&ty));
                    env.uni.constrain_row(row, activation.clone());
                    bare
                })
                .collect::<Vec<_>>();
            env.pending_pars
                .push(crate::env::PendingPar { span: e.span, components: types.clone() });
            Some(Type::rowed(Type::Par(types), activation))
        }
        // A bundle of exits: every component is supplied, and whoever
        // holds it takes exactly one — the additive conjunction. An item is a
        // by-name position, so one that ends in a cut is delayed and runs
        // only when it is chosen.
        Expr::Bundle(items) => {
            note_by_name(items, enums, env);
            let types =
                items.iter().map(|item| check_by_name(item, enums, env, diags)).collect::<Vec<_>>();
            types.into_iter().collect::<Option<Vec<_>>>().map(Type::With)
        }
        Expr::Pair(items) if items.is_empty() => Some(Type::ONE),
        Expr::Pair(items) => {
            note_by_name(items, enums, env);
            items
                .iter()
                .map(|item| check_by_name(item, enums, env, diags))
                .collect::<Option<Vec<_>>>()
                .map(packed_group)
        }
        Expr::Block(exprs) => {
            let mut result = None;
            env.push();
            for expr in exprs {
                if let Expr::Let { pattern, ty, value, body: None, mode } = &expr.kind {
                    // A bodyless `let` scopes over the rest of the block; the
                    // binding itself is checked exactly as the expression form.
                    let binding_ty =
                        check_let_binding(pattern, ty, value, *mode, enums, env, diags);
                    bind_let_pattern(pattern, binding_ty, value, enums, env);
                } else {
                    result = check_expr(expr, enums, env, diags);
                }
            }
            env.pop();
            result
        }
        // `a | b | c` — flow. The brackets say what the chain is, so
        // nothing is guessed: `<` makes the first stage a value, `>` makes
        // the last one consume, and every other step composes. A chain
        // that does not begin with a value denotes one that would, read as
        // `λx. x | …`, which gives composition its type for free —
        // `arrow(x, (;))` is a consumer, `arrow(x, B)` a function.
        Expr::Flow { stages, from_value, into_consumer } => {
            // What flows in is the argument the chain applies, or the value
            // it cuts: a by-name position.
            if *from_value && stages.len() >= 2 {
                note_by_name(&stages[..1], enums, env);
            }
            let yielding_row =
                !into_consumer && stages.len() >= 3 && is_command(&stages[stages.len() - 2], env);
            if yielding_row {
                note_by_name(&stages[stages.len() - 1..], enums, env);
            }
            let types: Vec<Option<Type>> = stages
                .iter()
                .enumerate()
                .map(|(index, stage)| {
                    let ty = if (*from_value && index == 0 && stages.len() >= 2)
                        || (yielding_row && index + 1 == stages.len())
                    {
                        check_by_name(stage, enums, env, diags)
                    } else {
                        check_expr(stage, enums, env, diags)
                    };
                    ty.map(|ty| env.uni.apply(&ty))
                })
                .collect();
            // `<` marks what flows in. Without it the head is a function and
            // the chain composes, whatever the head is: a head that cannot be
            // one is refused, pointing at the missing `<`.
            let opens = !from_value;
            if opens && let Some(head) = types.first().and_then(|ty| ty.as_ref()) {
                match &type_shape(head.clone()) {
                    Type::Par(parts) if !parts.is_empty() => {}
                    Type::Var(_) => {
                        let function = Type::Par(vec![env.uni.fresh_var(), env.uni.fresh_var()]);
                        let _ = env.uni.unify(head, &function);
                    }
                    other => {
                        diags.push(Diagnostic {
                            message: format!(
                                "a chain without `<` begins with a function, and this has type \
                                 {other}; send it as a value: `<… | …`"
                            ),
                            span: stages[0].span,
                        });
                        return None;
                    }
                }
            }
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
            let mut commuted: Vec<usize> = Vec::new();
            let turned: Vec<(usize, usize)> = Vec::new();
            let mut row_stage: Option<usize> = None;
            let mut yielding: Option<usize> = None;
            let swap = None;
            for (index, ty) in types.iter().enumerate().skip(usize::from(!opens)) {
                let last = index + 1 == types.len();
                let unknown = env.uni.fresh_var();
                let ty = ty.as_ref().unwrap_or(&unknown);
                let shape = flowing.unwrap_or(&e.kind);
                // A command takes two groups, so a chain hands it both:
                // what flows in is its values, and the rest of the chain —
                // the closing stage — is its menu of exits. The chain ends
                // there, in a call, and its type is `(;)`.
                if index + 2 == types.len()
                    && let Expr::Ident(name) = &stages[index].kind
                    && env.lookup(name).is_none()
                    && !env.traits.is_method(name)
                    && let Some(signature) = env.functions.get(name)
                    && signature.continuations.iter().any(|c| *c)
                    // Only a command: it answers `(;)`, so the chain ends
                    // in it. A negative function also carries a continuation
                    // but answers a consumer, and composes on — that is the
                    // commuted `;` reading below.
                    && signature.result.as_ref() == Some(&Type::BOTTOM)
                {
                    let (signature, seen) = instantiate(signature, &mut env.uni);
                    env.perform(signature.row.clone());
                    let split = signature.continuations.iter().filter(|c| !**c).count();
                    let values = packed_group(signature.params[..split].iter().cloned());
                    let row = exit_row(signature.params[split..].iter().cloned());
                    if !fits(env, &values, &acc, shape) {
                        let values = env.uni.apply(&values);
                        diags.push(Diagnostic {
                            message: format!(
                                "`{name}` takes {values}, and what flows in has type {acc}"
                            ),
                            span: stages[index].span,
                        });
                    }
                    let mut exits = types[index + 1].clone();
                    let mut answer = Type::BOTTOM;
                    if !into_consumer {
                        let (consumers, result, count) = yielding_exit_menu(
                            &row,
                            exits.clone()?,
                            stages[index + 1].span,
                            env,
                            diags,
                        )?;
                        exits = Some(consumers);
                        answer = result;
                        yielding = Some(count);
                    }
                    // Each exit of a bundle meets its declared exit as a
                    // value meets a slot: its row inside the declared one,
                    // not equal to it.
                    let menu_fits = |env: &mut Env, exits: &Type| {
                        let written = match &stages[index + 1].kind {
                            Expr::Bundle(items) => Some(items),
                            _ => None,
                        };
                        match (env.uni.apply(&row), env.uni.apply(exits)) {
                            (Type::With(declared), Type::With(items))
                                if declared.len() == items.len() =>
                            {
                                declared.iter().zip(&items).enumerate().all(
                                    |(position, (declared, item))| {
                                        let shape = written
                                            .and_then(|items| items.get(position))
                                            .map(|item| &item.kind)
                                            .unwrap_or(&stages[index + 1].kind);
                                        fits(env, declared, item, shape)
                                    },
                                )
                            }
                            _ => fits(env, &row, exits, &stages[index + 1].kind),
                        }
                    };
                    if let Some(exits) = exits.as_ref()
                        && !menu_fits(env, exits)
                    {
                        let row = env.uni.apply(&row);
                        diags.push(Diagnostic {
                            message: format!(
                                "`{name}` offers the exits {row}, and this menu has type {exits}"
                            ),
                            span: stages[index + 1].span,
                        });
                    }
                    // A bounded command takes its dictionaries first, as a
                    // bounded function does.
                    record_signs(&signature, &seen, name, stages[index].span, env);
                    if !signature.bounds.is_empty() {
                        env.pending_dicts.push(crate::env::PendingDicts {
                            span: stages[index].span,
                            callee: name.clone(),
                            bounds: pending_bounds(&signature, &seen),
                        });
                    }
                    row_stage = Some(index);
                    acc = answer;
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
                        // No parameters is the empty product: `(,) | f` is
                        // how a nullary declaration is called.
                        let packed = packed_group(fresh.params.iter().cloned());
                        // An alternative whose sum is not known yet fits any
                        // parameter; where only the other reading takes a
                        // sum, the general arm reads it that way.
                        let reads_back = is_pending_injection(env, &acc)
                            && sum_alternatives(&probe.apply(&packed)).is_none()
                            && fresh.result.as_ref().is_some_and(|result| {
                                sum_alternatives(&probe.apply(&result.dual())).is_some()
                            });
                        let probe = Env { uni: probe, ..env.clone() };
                        let piecewise = fits_piecewise(&probe, &fresh.params, &acc, shape);
                        !reads_back
                            && (piecewise || would_fit(&probe, &packed, &acc, Some(shape)))
                    }
                {
                    let (signature, seen) = instantiate(signature, &mut env.uni);
                    env.perform(signature.row.clone());
                    let packed = packed_group(signature.params.iter().cloned());
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
                            && items
                                .iter()
                                .zip(components.iter())
                                .zip(signature.params.iter())
                                .enumerate()
                                .all(|(position, ((item, actual), param))| {
                                    let rows_from = env.uni.row_constraints().len();
                                    let fitted = fits_turning(env, param, actual, item);
                                    let origin = argument_origin(name, &signature, position, item);
                                    env.tag_rows_since(rows_from, origin);
                                    fitted
                                })
                    });
                    let fitted = piecewise || {
                        let rows_from = env.uni.row_constraints().len();
                        let fitted = fits(env, &packed, &acc, shape);
                        if index == 1 {
                            let origin = argument_origin(name, &signature, 0, &stages[0]);
                            env.tag_rows_since(rows_from, origin);
                        }
                        fitted
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
                    record_signs(&signature, &seen, name, stages[index].span, env);
                    if !signature.bounds.is_empty() {
                        env.pending_dicts.push(crate::env::PendingDicts {
                            span: stages[index].span,
                            callee: name.clone(),
                            bounds: pending_bounds(&signature, &seen),
                        });
                    }
                    acc = signature.result.map(|ty| normalize(&ty, env)).unwrap_or(Type::ONE);
                    flowing = None;
                    continue;
                }
                // A trait method takes what flows in as its receiver, and
                // dispatches on it exactly as a call would.
                if !(last && *into_consumer)
                    && let Expr::Ident(name) = &stages[index].kind
                    && env.traits.is_method(name)
                    && let Some((result, turned)) =
                        check_method_stage(name, &acc, shape, stages[index].span, enums, env, diags)
                {
                    if turned {
                        commuted.push(index);
                    }
                    acc = result;
                    flowing = None;
                    continue;
                }
                // The closing stage consumes; every other one is a function.
                if last && *into_consumer {
                    env.pending_consumers.push((stages[index].span, ty.clone()));
                    // The orientation rule: a consumer stands only at the
                    // right end, so one arriving from the left is the
                    // mistake. A function and codata are negative *values*
                    // and flow in like any other.
                    let bare = type_shape(acc.clone());
                    // A variable counts once its sign says it is a consumer.
                    if bare != Type::BOTTOM
                        && bare.is_negative()
                        && !matches!(bare, Type::Par(ref parts) if !parts.is_empty())
                        && (!contains_var(&bare)
                            || type_polarity(&bare, env) == Some(ParamPolarity::Negative))
                        && !enums.is_negative_value(&bare)
                        && type_polarity(ty, env) != Some(ParamPolarity::Positive)
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "the left of `|` is the value side, and this has type {acc}; \
                                 pass a continuation as an argument instead"
                            ),
                            span: stages[index].span,
                        });
                        return Some(Type::BOTTOM);
                    }
                    // Feeding the consumer runs it: it performs its row — a
                    // bundle's, item by item — and a form's declared latent
                    // row.
                    let consumer = open_exit(env.uni.apply(ty), env);
                    if let Type::Dual(inner) = &consumer
                        && let Type::Named(form, form_args) = inner.as_ref()
                        && let Some(latent) = latent_row(enums, form, form_args, env)
                    {
                        env.perform(latent);
                    }
                    // A function closed with `>` is the slip of writing
                    // `<v | resume>` for `<v | resume`: a function applied by
                    // a cut takes its argument and a continuation together,
                    // and the mismatch that makes says nothing of the `>`.
                    // A consumer of a pair holding a continuation, `k: (A, -B)`,
                    // has a function's type too, `(-A ; B)`; a pair flowing
                    // in is then a pair meeting a pair, not a function
                    // closed with `>`.
                    let pair_flows_in = matches!(
                        type_shape(env.uni.apply(&acc)),
                        Type::Tensor(items) if items.len() == 2
                    );
                    let function = match (&stages[index].kind, &consumer) {
                        (Expr::Ident(name), Type::Par(parts))
                            if parts.len() == 2 && !pair_flows_in =>
                        {
                            let result = &parts[1];
                            let returns = if contains_var(result) {
                                type_polarity(result, env) != Some(ParamPolarity::Negative)
                            } else {
                                result.is_positive()
                            };
                            returns.then_some(name)
                        }
                        _ => None,
                    };
                    let closed_function = |name: &str| Diagnostic {
                        message: format!(
                            "`{name}` is a function, and `>` delivers to a consumer; apply it \
                             by leaving the `>` off: `<… | {name}`"
                        ),
                        span: stages[index].span,
                    };
                    // An alternative flowing in would take the function's
                    // pair as its sum, and be reported for that instead.
                    if let Some(name) = function
                        && is_pending_injection(env, &acc)
                    {
                        diags.push(closed_function(name));
                        acc = Type::BOTTOM;
                        continue;
                    }
                    let expects = consumer.dual();
                    // The `(;)`/`(,)` corner: `-(;)` resolves to `(,)`, so the
                    // idiomatic `<(,) | k>` is unit meeting unit.
                    let units = acc == Type::ONE && ty == &Type::ONE;
                    let before = env.uni.clone();
                    if !units && !fits(env, &expects, &acc, shape) {
                        env.uni = before;
                        if let Some(name) = function {
                            diags.push(closed_function(name));
                        } else {
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
                    acc = Type::BOTTOM;
                    continue;
                }
                // A function: what flows in is its argument, its result
                // flows on. `;` is commutative, so a stage `(A ; B)` reads
                // both ways — `(dual(A) -> B)` and `(dual(B) -> A)` — and the two
                // styles meet here: `area: Shape -> i64` and
                // `area_of(out: -i64) <- Shape` are one type, so either
                // stands in a pipeline. What flows in picks the reading,
                // and where both fit they agree.
                // A declared callee with a value group has had its chance in
                // the signature arm, which supplies the whole group or
                // nothing. Reaching here means what flows in is only part of
                // it: `;` is associative, so the callee's type presents its
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
                        many => format!("({})", many.join(", ")),
                    };
                    let is_command = signature.continuations.iter().any(|c| *c);
                    // The values may all be there, with only the exits
                    // missing: then the chain is what is short, not the group.
                    let values_fit = {
                        let mut probe = env.uni.clone();
                        let (fresh, _) = instantiate(signature, &mut probe);
                        let packed = packed_group(
                            fresh
                                .params
                                .iter()
                                .zip(&fresh.continuations)
                                .filter(|(_, is_row)| !**is_row)
                                .map(|(ty, _)| ty.clone()),
                        );
                        let probe = Env { uni: probe, ..env.clone() };
                        would_fit(&probe, &packed, &acc, Some(shape))
                    };
                    let message = if is_command && values_fit {
                        format!(
                            "`{name}` is a proc: after its values it takes its menu of \
                             exits, so the chain closes on them — `<… | {name} | (…)>`"
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
                // A bounded function has no type as a value, since its
                // dictionaries would have to travel with it; named as a stage
                // it is given its signature's, and the dictionaries its bounds
                // need are recorded at the stage.
                let signed;
                let ty = match &stages[index].kind {
                    Expr::Ident(name)
                        if env.lookup(name).is_none()
                            && !env.traits.is_method(name)
                            && env.functions.get(name).is_some_and(|s| !s.bounds.is_empty()) =>
                    {
                        let signature = env.functions.get(name).expect("checked above");
                        let (signature, seen) = instantiate(signature, &mut env.uni);
                        record_signs(&signature, &seen, name, stages[index].span, env);
                        env.pending_dicts.push(crate::env::PendingDicts {
                            span: stages[index].span,
                            callee: name.clone(),
                            bounds: pending_bounds(&signature, &seen),
                        });
                        signed = match signature.result {
                            Some(result) if !signature.params.is_empty() => Type::rowed(
                                Type::arrow(packed_group(signature.params), result),
                                signature.row,
                            ),
                            Some(result) => result,
                            None => env.uni.fresh_var(),
                        };
                        &signed
                    }
                    _ => ty,
                };
                // Applying the stage runs it: it performs its row.
                let ty = activation_type(ty.clone(), env);
                let left = env.uni.fresh_var();
                let right = env.uni.fresh_var();
                let par = Type::Par(vec![left.clone(), right.clone()]);
                if env.uni.unify(&ty, &par).is_err() {
                    diags.push(Diagnostic {
                        message: format!(
                            "a step composes, so this stage is a function; it has type {ty}. \
                             To deliver to it instead, close the chain: `<… | consumer>`"
                        ),
                        span: stages[index].span,
                    });
                    return None;
                }
                let (left, right) = (env.uni.apply(&left), env.uni.apply(&right));
                let forward = left.dual();
                // An alternative flowing in has a sum only its context names,
                // so the unknown would fit the forward reading's argument
                // whatever it is. Where only the mirrored reading takes a sum —
                // `<::0(7) | describe` with `describe(out: String) <- (i64 |
                // String)` — that is the reading the alternative meets.
                let mirrored_first = is_pending_injection(env, &acc)
                    && sum_alternatives(&env.uni.apply(&forward)).is_none()
                    && sum_alternatives(&env.uni.apply(&right.dual())).is_some();
                if !mirrored_first && fits(env, &forward, &acc, shape) {
                    acc = env.uni.apply(&right);
                } else {
                    let mirrored = right.dual();
                    commuted.push(index);
                    if !fits(env, &mirrored, &acc, shape) {
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
            // `>` is syntax, but "does a function head this chain?" is not,
            // so lowering is told.
            env.dispatch.flows.insert(
                e.span,
                slc_syntax::lower::FlowShape {
                    eta: opens,
                    cut: *into_consumer,
                    commuted,
                    row_stage,
                    yielding,
                    swap,
                    turned,
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
                    (None, Some(name)) => infer_param_type(name, body, enums, env).map(type_shape),
                    (None, None) => None,
                }
                .unwrap_or_else(|| env.uni.fresh_var().dual());
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
        // Printing lives in the prelude, which these checks do not load; a
        // stand-in is appended, so no diagnostic's position moves.
        let s =
            &format!("{s}\nfunc println<+T>(x: T) -> (,) {{ (,) }}\nenum Bool {{ False, True }}\n");
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        let (prog, traits) = slc_syntax::traits::elaborate(&prog).expect("elaborate");
        check_program(&prog, &traits)
    }

    #[test]
    fn a_command_body_must_be_bottom() {
        // A bare value reaches no continuation.
        let diags = check("proc bad(x: +i32) | (k: -i32) { x }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must reach a continuation")), "{diags:?}");
        // An arm that yields a value falls through on its path.
        let diags =
            check("proc bad(x: +i32) | (k: -i32) { of __eq(x, 0) { True => <x | k>, _ => (,) } }")
                .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must reach a continuation")), "{diags:?}");
    }

    #[test]
    fn a_command_may_leave_a_continuation_unused() {
        // The core is classical: reaching one continuation is enough, so a
        // declared continuation the body never triggers is not an error.
        assert!(
            check("proc f(x: +i32) | (ok: -i32 & err: -i32) { <x | ok> }").is_ok(),
            "dropping a continuation should be allowed"
        );
    }

    #[test]
    fn projection_resolves_and_range_checks() {
        // A tuple component and a record field both check.
        assert!(
            check(
                "data P { x: +i64, y: +i64 }
                 func f(p: +P) -> i64 { (<(p.x, p.y) | __add) }
                 func g(t: (+i64, +i64, +i64)) -> i64 { (<(t.0, t.2) | __add) }
                 func h(t: (+i64, (+i64, +i64))) -> (+i64, +i64) { t.1 }"
            )
            .is_ok()
        );
        // Out of range.
        let diags = check("func f(t: (+i64, +i64)) -> i64 { t.5 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("out of range")), "{diags:?}");
        // Unknown field.
        let diags = check("data P { x: +i64 } func f(p: +P) -> i64 { p.y }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no field `y`")), "{diags:?}");
    }

    #[test]
    fn record_literal_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "data Direction { left: i64, right: i64 }
                 func use_it(d: Direction) -> i64 { 0 }
                 func f() -> i64 { <Direction { left: 1, right: 2 } | use_it }"
            )
            .is_ok()
        );

        let missing = check(
            "data Direction { left: i64, right: i64 }
             func f() -> i64 { use_it(Direction { left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            missing.iter().any(|d| d.message.contains("the literal writes `left`")),
            "{missing:?}"
        );

        let reordered = check(
            "data Direction { left: i64, right: i64 }
             func f() -> i64 { use_it(Direction { right: 2, left: 1 }) }",
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
                 func f(d: D) -> i64 {
                     of d {
                         D { left: a, right: b } => a,
                         _ => 0,
                     }
                 }"
            )
            .is_ok()
        );

        let diags = check(
            "data D { left: i64, right: i64 }
             func f(d: D) -> i64 {
                 of d {
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
             func f(d: D) -> i64 {
                 of d {
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
             func f(x: +i64) -> i64 {
                 of x {
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
             func f() -> i64 { use_it(Direction { left: 1, right: \"two\" }) }",
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
        let diags = check("func f() -> i64 { use_it(Nope { a: 1 }) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`Nope` is not a declared record")),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_variant_without_payload_is_a_value() {
        assert!(check("enum Color { Red } func f() -> Color { Color::Red }").is_ok());
        assert!(check("enum Color { Red } func f() -> Color { Red }").is_ok());
    }

    #[test]
    fn enum_variant_with_payload_is_a_value_only_when_applied() {
        // An integer literal has type `+i64`.
        assert!(check("enum R { Some(i64) } func f() -> R { R::Some(1) }").is_ok());
        let diags = check("enum R { Some(i64) } func f() -> R { R::Some }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("carries 1 payload value(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_variant_payload_arity_is_checked() {
        let diags = check("enum R { Both(i64, i64) } func f() -> R { R::Both(1) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the expression supplies 1")), "{diags:?}");
    }

    #[test]
    fn enum_variant_payload_type_is_checked() {
        let diags = check("enum R { Some(i64) } func f() -> R { R::Some(\"text\") }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("payload of `R::Some` has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn declaration_name_is_not_a_value() {
        for source in [
            "enum Color { Red } func f() -> Color { Color }",
            "data S { a: i32 } func f() -> i32 { S }",
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
                 func k(ok: -i64 & absent: -i64) <- R {
                     mu R {
                         Some(value) => <value | ok>,
                         None => <0 | absent>,
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
             func k(ok: -i64 & absent: -i64) <- R {
                 mu R {
                     Some => <0 | ok>,
                     None => <0 | absent>,
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
             func k(absent: -i32) <- R {
                 mu R {
                     None(value) => <0 | absent>,
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
    fn a_chain_without_an_opening_bracket_begins_with_a_function() {
        // A value flows in only when `<` marks it.
        let diags = check("func shout(k: -String) -> (;) { \"hi\" | k> }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("a chain without `<` begins with a function")
                && d.message.contains("+String")),
            "{diags:?}"
        );
        assert!(check("func shout(k: -String) -> (;) { <\"hi\" | k> }").is_ok());
        // Without it, a function heads the chain and composes.
        assert!(
            check(
                "func double(n: i64) -> i64 { (<(n, 2) | __mul) }
                 func quadruple(n: i64) -> i64 { <n | (double | double) }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_continuation_is_cut_against_not_called() {
        let diags = check("proc route(x: +i32) | (k: -i32) { k(x) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("is a consumer of type -i32, not a function")
                && d.message.contains("<value | k>")),
            "{diags:?}"
        );
        assert!(check("proc route(x: +i32) | (k: -i32) { <x | k> }").is_ok());
    }

    #[test]
    fn exit_is_a_continuation() {
        let diags = check("proc f | (out: -i32) { out(0) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`out` is a consumer of type -i32")),
            "{diags:?}"
        );
        assert!(check("proc f | (out: -i32) { <0 | out> }").is_ok());
    }

    #[test]
    fn a_cut_is_a_command() {
        // A cut has type `(;)`: it produces nothing and control does not return,
        // so an arm that ends in one leaves the `of` type to the other.
        let ok = check(
            "func parse(input: +String, err: -String) -> i64 {
                 of (<(str_len(input), 0) | __gt) { True => 1, _ => <\"empty\" | err> }
             }",
        );
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn a_cut_needs_dual_sides() {
        // Two values of the same positive type do not interact.
        let diags = check("func f(x: +i32, y: +i32) -> i32 { <x | y> }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("what flows in has type")), "{diags:?}");
    }

    #[test]
    fn a_cut_checks_what_the_consumer_accepts() {
        let diags = check("proc route(x: +String) | (k: -i32) { <x | k> }").unwrap_err();
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
                 func code(return: -i64) <- Color {
                     mu Color {
                         Red => <0 | return>,
                         Green => <1 | return>,
                     }
                 }
                 func main() -> i64 {
                     mu i64 { answer <= <Color::Green | code | answer> }
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_accepts_the_declared_row() {
        assert!(
            check(
                "proc route(x: +i32) | (k: -i32) { <x | k> }
                 func main() -> i32 { mu i32 { out <= <1 | route | out> } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_rejects_an_incompatible_continuation_type() {
        let diags = check(
            "proc route(x: +i32) | (k: -i32) { <x | k> }
             func main() -> i32 { mu Bool { out <= route(1, out) } }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("continuation row mismatch")
                && d.message.contains("dual(Bool)")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn continuation_row_is_positional() {
        // The row is ordered: swapping two continuations of different types
        // is rejected even though both types appear in the declaration.
        let diags = check(
            "proc route(a: -i32, b: -Bool) | (c: -i32 & d: -Bool) { <0 | c> }
             proc caller | (first: -i32 & second: -Bool) { route(0, True, second, first) }",
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
            "proc route(x: +i32) | (k: -i32) { <x | k> }
             func main() -> i32 { mu i32 { out <= route(1, out, out) } }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("continuation row of 1")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn arithmetic_type_mismatch_rejected() {
        let diags = check("func f(a: +i32, b: +i64) -> i64 { (<(a, b) | __add) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("what flows in has type")), "{diags:?}");
    }

    #[test]
    fn comparison_char_and_int_mismatch_rejected() {
        let diags = check("func f(a: +char, b: +i32) -> Bool { (<(a, b) | __lt) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("what flows in has type")), "{diags:?}");
    }

    #[test]
    fn char_comparison_ok() {
        assert!(check("func f(a: +char, b: +char) -> Bool { (<(a, b) | __lt) }").is_ok());
    }

    #[test]
    fn string_concat_ok() {
        assert!(check(r#"func f(a: +String, b: +String) -> String { (<(a, b) | __add) }"#).is_ok());
    }

    #[test]
    fn a_bundle_item_that_ends_in_a_cut_is_delayed() {
        // An item is a by-name position: a jump in one is delayed, and fires
        // only when it is chosen — whether the row declares `(;)` or `-(,)`.
        for row in ["(;)", "-(,)"] {
            let src = format!(
                "proc choose(c: Bool) | (then: {row} & otherwise: {row}) {{
                     of c {{ True => <(,) | then>, _ => <(,) | otherwise> }}
                 }}
                 proc main | (exit: -i32) {{
                     <(1, 0) | __lt | choose | ({{ <0 | exit> }} & {{ <1 | exit> }})>
                 }}"
            );
            assert!(check(&src).is_ok(), "{row}: {:?}", check(&src));
        }
        // The consumers the items meant are accepted.
        assert!(
            check(
                "proc choose(c: Bool) | (then: -(,) & otherwise: -(,)) {
                     of c { True => <(,) | then>, _ => <(,) | otherwise> }
                 }
                 proc main | (exit: -i32) {
                     <(1, 0) | __lt | choose | (fn(_) { <0 | exit> } & fn(_) { <1 | exit> })>
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_builtin_stage_is_held_to_its_signature() {
        // What flows into a builtin is checked the way it is for a declared
        // function: `add` takes two integers, and the whole group at once.
        for (body, expected) in [
            (r#"let n = <(1, "b") | __add;"#, "what flows in"),
            ("let inc = <1 | __add;", "not applied to part of a group"),
        ] {
            let diags =
                check(&format!("proc main | (exit: -i32) {{ {body} <0 | exit> }}")).unwrap_err();
            assert!(diags.iter().any(|d| d.message.contains(expected)), "{body}: {diags:?}");
        }
        assert!(check("proc main | (exit: -i32) { let n = <(1, 2) | __add; <0 | exit> }").is_ok());
    }

    #[test]
    fn char_pattern_checked() {
        let diags = check("func f(c: +char) -> i64 { of c { 3 => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type")));
    }

    #[test]
    fn index_type_checked() {
        let diags =
            check("func f(s: +String, i: +Bool) -> char { (<(s, i) | __index) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("what flows in has type")), "{diags:?}");
    }

    #[test]
    fn typed_let_checked() {
        let diags = check("func f() -> i32 { let x: +char = 1; 2 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("annotated")));
    }

    #[test]
    fn const_type_checked() {
        let diags = check("def X: +char = 1;").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("initializer")));
    }

    #[test]
    fn non_constant_const_initializer_rejected() {
        let diags = check("def X: +i32 = __add(1, 2);").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("literal or another definition")));
    }

    #[test]
    fn range_endpoints_checked() {
        let diags = check("func f(c: +char) -> i64 { of c { 'a'..=3 => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type")));
    }

    #[test]
    fn select_expression_has_dual_named_type() {
        let r = check(
            "enum Color { Red, Green, Blue }
            func k(return: -i32) <- Color {
                mu Color {
                    Red => <0 | return>,
                    Green => <1 | return>,
                    Blue => <2 | return>,
                }
            }",
        );
        assert!(r.is_ok(), "unexpected diagnostics: {r:?}");
    }

    #[test]
    fn select_arm_must_be_a_command() {
        let diags = check(
            "enum Color { Red, Green }
            func k(return: -i32) <- Color {
                mu Color {
                    Red => 0,
                    Green => <1 | return>,
                }
            }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`mu` arm is a command")), "{diags:?}");
    }

    #[test]
    fn select_arm_must_cover_a_shape_of_the_type() {
        let diags = check(
            "enum Color { Red, Green }
            func k(return: -i32) <- Color {
                mu Color {
                    (a, b) => <0 | return>,
                    Green => <1 | return>,
                }
            }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must cover a shape of")), "{diags:?}");
    }

    #[test]
    fn unit_is_a_type_and_not_a_wildcard() {
        let diags = check(
            "func wants(x: +String) -> i64 { 0 }
             proc main | (exit: -i32) / {IO} { <wants((,)) | println; <0 | exit> }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type (,)")),
            "unit fit everything once: {diags:?}"
        );
    }

    #[test]
    fn a_return_type_must_name_a_declared_type() {
        for source in [
            "func f() -> Foo { (,) }",
            "func f(out: i64) <- Foo { mu Foo {} }",
            "func f() -> i64 { let g = fn(x: i64) -> Foo { x }; 0 }",
            "hook E { func ask(x: i64) -> Foo; }",
            "spec T { func m(self: Self) -> Foo; }",
        ] {
            let diags = check(source).unwrap_err();
            assert!(
                diags
                    .iter()
                    .any(|d| d.message.contains("names `Foo`, which is not a declared type")),
                "{source}: {diags:?}"
            );
        }
        // A type parameter, `Self`, and a sum over a type parameter all name
        // something.
        assert!(
            check(
                "spec T { func m(self: Self) -> Self; }
                 func g<+A>(x: A) -> (A | i64) { ::0(x) }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_written_type_in_a_declaration_or_annotation_must_name_a_declared_type() {
        for (source, expected) in [
            ("data D { x: Foo }", "the type of field `x` of `D`"),
            ("form F { x: Foo }", "the type of field `x` of `F`"),
            ("menu M { item: Foo }", "the type of item `item` of `M`"),
            ("enum E { A(i64, Foo) }", "payload 1 of `E::A`"),
            ("func f() -> i64 { let x: Foo = 1; 0 }", "the annotation of `let x`"),
        ] {
            let diags = check(source).unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains(expected)
                    && d.message.contains("names `Foo`, which is not a declared type")),
                "{source}: {diags:?}"
            );
        }
        // A declaration's own type parameters, and itself, are names it may use.
        assert!(
            check(
                "data Box<+T> { value: T }
                 form Put<+T> { put: T }
                 menu Get<+T> { get: T }
                 enum Chain<+T> { Link(T, Chain<T>), End }
                 func f() -> i64 { let x: Box<i64> = Box { value: 1 }; 0 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn the_units_are_the_nullary_connectives() {
        assert!(
            check(
                "func unit_value() -> (,) { (,) }
                 func top_value() -> (&) { (&) }
                 func absurd(out: -i64) <- (|) { mu (|) {} }
                 proc halt | (exit: -i32) -> (;) { <0 | exit> }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_declaration_named_like_a_unit_is_an_ordinary_declaration() {
        assert!(check("data Unit { value: i64 } func f() -> Unit { Unit { value: 1 } }").is_ok());
        assert!(check("data Unit {} func not_unit() -> Unit { (,) }").is_err());
    }

    #[test]
    fn a_handler_may_resume_once_in_any_position() {
        // Work after a single resume is fine now — the continuation is one
        // stack, so the resumed result flows back into the clause.
        assert!(
            check(
                "hook Ask { func ask() -> i64; }
                 func u() -> i64 / {Ask} { (<(ask(), 5) | __add) }
                 proc main | (exit: -i32) / {IO} {
                     let r = do u() hn { ask(): resume => (<(1000, resume(7)) | __add), return(n) => n };
                     <r | println; <0 | exit>
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
                "hook C { func c() -> Bool; }
                 func f() -> i64 / {C} { of c() { True => 1, _ => 2 } }
                 proc main | (exit: -i32) / {IO} {
                     let r = do f() hn {
                         c(): resume => (<(resume(True), resume(False)) | __add),
                         return(n) => n,
                     };
                     <r | println; <0 | exit>
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_trait_parameter_is_solved_by_the_type_the_call_produces() {
        assert!(
            check(
                "enum Wrap { Held(i64) }
                 spec Into<+U> { func into(self: Self) -> U; }
                 impl Into<i64> for Wrap {
                     func into(self: Wrap) -> i64 { of self { Held(n) => n } }
                 }
                 impl Into<String> for Wrap {
                     func into(self: Wrap) -> String {
                         of self { Held(n) => <n | int_to_str }
                     }
                 }
                 func number(w: Wrap) -> i64 { <w | into }
                 func text(w: Wrap) -> String { <w | into }
                 func to_text<+T: Into<String>>(x: T) -> String { <x | into }
                 proc main | (exit: -i32) / {IO} { <0 | exit> }"
            )
            .is_ok()
        );
    }

    #[test]
    fn an_unconstrained_trait_parameter_is_refused() {
        let diags = check(
            "enum Wrap { Held(i64) }
             spec Into<+U> { func into(self: Self) -> U; }
             impl Into<i64> for Wrap {
                 func into(self: Wrap) -> i64 { of self { Held(n) => n } }
             }
             func ambiguous(w: Wrap) -> i64 { let x = <w | into; 0 }
             proc main | (exit: -i32) / {IO} { <0 | exit> }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("needs a type for `U`")), "{diags:?}");
    }

    #[test]
    fn a_trait_method_dispatches_and_bounds_discharge() {
        assert!(
            check(
                "spec Show { func show(self: +Self) -> String; }
                 impl Show for i64 { func show(self: +i64) -> String { <self | int_to_str } }
                 func label<+T: Show>(x: +T) -> String { <x | show }
                 proc main | (exit: -i32) / {IO} { <1 | label | println; <0 | exit> }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_monomorphic_method_call_resolves_for_static_dispatch() {
        // `show(1)` has a concrete receiver, so it resolves to the i64 impl;
        // `show(x)` under `<T: Show>` stays dynamic (the map does not name it).
        let resolve = |s: &str| {
            // The prelude's printing, stood in for as `check` does.
            let s = &format!(
                "{s}\nfunc println<+T>(x: T) -> (,) {{ (,) }}\nenum Bool {{ False, True }}\n"
            );
            let toks = lex(s).unwrap();
            let prog = parse(toks).unwrap();
            let (prog, traits) = slc_syntax::traits::elaborate(&prog).expect("elaborate");
            check_program_resolving(&prog, &traits).expect("checks")
        };
        let mono = resolve(
            "spec Show { func show(self: +Self) -> String; }
             impl Show for i64 { func show(self: +i64) -> String { <self | int_to_str } }
             proc main | (exit: -i32) / {IO} { <1 | show | println; <0 | exit> }",
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
            "spec Show { func show(self: +Self) -> String; }
             impl Show for i64 { func show(self: +i64) -> String { <self | int_to_str } }
             func label<+T: Show>(x: +T) -> String { <x | show }
             proc main | (exit: -i32) / {IO} { <1 | label | println; <0 | exit> }",
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
                "spec Show { func show(self: +Self) -> String; }
                 impl Show for i64 { func show(self: +i64) -> String { int_to_str(self) } }
                 func emit<+T: Show>(out: -String & v: +T) <- i64 { <show(v) | out> }"
            )
            .is_ok()
        );
        let diags = check(
            "spec Show { func show(self: +Self) -> String; }
             impl Show for i64 { func show(self: +i64) -> String { int_to_str(self) } }
             func emit<+T>(out: -String & v: +T) <- i64 { <show(v) | out> }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("not known to satisfy")), "{diags:?}");
    }

    #[test]
    fn a_method_with_no_impl_is_rejected() {
        let diags = check(
            "spec Show { func show(self: +Self) -> String; }
             impl Show for i64 { func show(self: +i64) -> String { int_to_str(self) } }
             proc main | (exit: -i32) / {IO} { <show(True) | println; <0 | exit> }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no `impl Show for Bool`")), "{diags:?}");
    }

    #[test]
    fn an_unbounded_generic_cannot_call_a_method() {
        let diags = check(
            "spec Show { func show(self: +Self) -> String; }
             impl Show for i64 { func show(self: +i64) -> String { int_to_str(self) } }
             func bad<+T>(x: +T) -> String { show(x) }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("not known to satisfy")), "{diags:?}");
    }

    #[test]
    fn a_use_gives_a_type_parameter_its_declared_polarity() {
        const ID: &str = "func id<+T>(x: T) -> T { x }\n";
        // A function is negative, and `id` holds positive types only.
        let diags =
            check(&format!("{ID}func f() -> i64 {{ let g = <(fn(y: i64) {{ y }}) | id; <1 | g }}"))
                .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`id` declares `<+T>`")
                && d.message.contains("negative type")),
            "{diags:?}"
        );
        // And the other way round.
        let diags = check("func k<-T>(x: T) -> T { x }\nfunc f() -> i64 { <1 | k }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`k` declares `<-T>`")
                && d.message.contains("positive type")),
            "{diags:?}"
        );
        // A generic body passes its own parameter on only where the marks agree.
        assert!(check(&format!("{ID}func f<+U>(x: U) -> U {{ <x | id }}")).is_ok());
        let diags = check(&format!("{ID}func f<-U>(x: U) -> U {{ <x | id }}")).unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`id` declares `<+T>`")), "{diags:?}");
    }

    #[test]
    fn a_construction_gives_a_type_parameter_its_declared_polarity() {
        let diags = check(
            "enum Held<+T> { Put(T) }
             func f() -> i64 { let held = Held::Put(fn(y: i64) { y }); 0 }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`Held` declares `<+T>`")), "{diags:?}");
    }

    #[test]
    fn a_delayed_let_binds_a_negative_computation_to_a_name() {
        assert!(check("func f() -> i64 { let- g = fn(y: i64) { y }; <1 | g }").is_ok());
        let diags = check("func f() -> i64 { let- n = 1; n }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("has the positive type")), "{diags:?}");
        let diags =
            check("func f() -> i64 { let- (a, b) = (fn(y: i64) { y }, 1); b }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`let-` binds a name")), "{diags:?}");
        // `let+` computes now whatever the type.
        assert!(check("func f() -> i64 { let+ n = 1; n }").is_ok());
    }

    #[test]
    fn a_method_of_two_parameters_reads_self_off_its_group() {
        const COMBINE: &str = "spec Combine { func combine(self: Self, other: Self) -> Self; }
             impl Combine for i64 { func combine(self: i64, other: i64) -> i64 { self } }
             impl Combine for i32 { func combine(self: i32, other: i32) -> i32 { self } }
             impl Combine for String { func combine(self: String, other: String) -> String { self } }\n";
        assert!(check(&format!("{COMBINE}func f() -> i64 {{ <(1, 2) | combine }}")).is_ok());
        assert!(
            check(&format!("{COMBINE}func f() -> String {{ <(\"a\", \"b\") | combine }}")).is_ok()
        );
        // A literal takes its width from the other operand.
        assert!(check(&format!("{COMBINE}func f(x: i32) -> i32 {{ <(1, x) | combine }}")).is_ok());
        let diags = check(&format!("{COMBINE}func f(x: i32) -> i32 {{ <(x, \"s\") | combine }}"))
            .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("as parameter 2")), "{diags:?}");
    }

    #[test]
    fn a_let_of_a_value_generalizes() {
        // One binding, three instantiations — through an alias, too: an
        // identifier is a value form.
        assert!(
            check(
                "enum Maybe<+T> { Nothing, Just(T) }
                 func or_else<+T>(m: Maybe<T>, fallback: T) -> T {
                     of m { Maybe::Just(x) => x, Maybe::Nothing => fallback }
                 }
                 proc main | (exit: -i32) / {IO} {
                     let nothing = Maybe::Nothing;
                     <(nothing, 1) | or_else | println;
                     <(nothing, \"s\") | or_else | println;
                     let alias = nothing;
                     <(alias, True) | or_else | println;
                     <0 | exit>
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_lambda_parameter_of_unknown_polarity_is_refused() {
        // A lambda nothing pins down no longer generalizes: its parameter's
        // polarity is unknown, and it asks for an annotation.
        let diags = check(
            "proc main | (exit: -i32) / {IO} {
                 let f = fn(x) { x };
                 <0 | exit>
             }",
        )
        .unwrap_err();
        assert_eq!(
            diags.iter().filter(|d| d.message.contains("the parameter `x`")).count(),
            1,
            "{diags:?}"
        );
        // A use that fixes the type is enough.
        assert!(
            check("func f() -> i64 { let g = fn(x) { (<(x, 1) | __add) }; <1 | g }").is_ok(),
            "{:?}",
            check("func f() -> i64 { let g = fn(x) { (<(x, 1) | __add) }; <1 | g }")
        );
    }

    #[test]
    fn the_value_restriction_keeps_computations_monomorphic() {
        // The Harper–Lillibridge weapon: a `mu` capture. Generalizing it
        // would let a continuation captured at one instantiation be re-used
        // at another, so it stays monomorphic and mixed uses are rejected.
        let diags = check(
            "proc main | (exit: -i32) / {IO} {
                 let g = mu { k <= <fn(x) { x } | k> };
                 <(g(1), 1) | __add | println;
                 <str_len(g(\"s\")) | println;
                 <0 | exit>
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");

        // An application is not a value either: it may run a command, and
        // its result may hold a captured continuation.
        let diags = check(
            "func id<+T>(x: T) -> T { x }
             proc main | (exit: -i32) / {IO} {
                 let h = id(fn(x) { x });
                 <(h(1), 1) | __add | println;
                 <str_len(h(\"s\")) | println;
                 <0 | exit>
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");
    }

    #[test]
    fn eta_expansion_reruns_the_capture() {
        // Wrapping the capture in a lambda makes it a value: each use
        // re-runs the capture.
        assert!(
            check(
                "proc main | (exit: -i32) / {IO} {
                     let fresh = fn(u: (,)) { mu { k <= <fn(x: i64) { x } | k> } };
                     <(fresh((,))(1), 1) | __add | println;
                     <(fresh((,))(2), 1) | __add | println;
                     <0 | exit>
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
            "proc main | (exit: -i32) / {IO} {
                 let g = fn(x) { x };
                 <(g(1), str_len(g(1))) | __add | println;
                 <0 | exit>
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("expected +String")),
            "closure calls went unchecked once: {diags:?}"
        );

        // The body constrains the parameter, and the call site honors it.
        let diags = check(
            "proc main | (exit: -i32) / {IO} {
                 <fn(x) { (<(x, 1) | __add) }(\"not a number\") | println;
                 <0 | exit>
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
                "func id<+T>(x: T) -> T { x }
                 proc main | (exit: -i32) / {IO} {
                     <((<42 | id), 1) | __add | println;
                     <\"each call its own T\" | id | str_len | println;
                     <0 | exit>
                 }"
            )
            .is_ok()
        );

        // Within one call, T is one type.
        let diags = check(
            "func id<+T>(x: T) -> T { x }
             proc main | (exit: -i32) / {IO} { <str_len(id(42)) | println; <0 | exit> }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("expected +String")), "{diags:?}");
    }

    #[test]
    fn a_body_produces_what_the_declaration_promises() {
        let diags = check("func f() -> i64 { \"not an i64\" }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the declaration says +i64")), "{diags:?}");
        // An integer literal still adapts to the declared width.
        assert!(check("func f() -> i32 { 0 }").is_ok());
        // A body that ends in a cut produces nothing, and promises nothing.
        assert!(check("func f(k: -i64) <- i64 { <1 | k> }").is_ok());
    }

    #[test]
    fn a_type_parameter_is_rigid_inside_the_body() {
        // `T` is whatever the caller chose, so the body may not treat it as
        // a number…
        let diags = check("func sneaky<+T>(x: T) -> T { (<(x, 1) | __add) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("what flows in has type")), "{diags:?}");

        // …or hand back some other parameter's type.
        let diags = check("func swap<+T, +U>(x: T, y: U) -> T { y }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the declaration says")), "{diags:?}");

        assert!(check("func id<+T>(x: T) -> T { x }").is_ok());
    }

    #[test]
    fn a_call_is_not_applied_to_part_of_its_group() {
        // A command given only some of its values was accepted — its
        // `;`-nested type presented the first parameter alone — and then
        // crashed at run time, where the group is bound as one argument.
        let route = "proc route(tag: String, x: i64) | (k: i64) { <x | k> }\n";
        for body in [r#"let h = <"high" | route; <0 | exit>"#, r#"<"high" | route; <0 | exit>"#] {
            let diags = check(&format!("{route}proc main | (exit: -i32) / {{IO}} {{ {body} }}"))
                .unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains("not applied to part of a group")),
                "{body}: {diags:?}"
            );
        }
        // A positive function likewise — and one named like a builtin, which
        // used to inherit every builtin exemption by name and slip past.
        let diags = check(
            "func add(a: i64, b: i64) -> i64 { (<(a, b) | __add) }
             proc main | (exit: -i32) / {IO} { let inc = <1 | __add; <0 | exit> }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("not applied to part of a group")),
            "{diags:?}"
        );
        // All the values and no exits is a command short of its chain, and
        // says so.
        let diags = check(
            "proc one(x: i64) | (k: i64) { <x | k> }
             proc main | (exit: -i32) / {IO} { let h = <1 | one; <0 | exit> }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`one` is a proc")), "{diags:?}");
        // The whole group still calls, and a negative function still reads
        // the mirrored way round.
        assert!(
            check(&format!(
                "{route}proc main | (exit: -i32) / {{IO}} {{
                     <mu i64 {{ k <= <(\"high\", 7) | route | k> }} | println; <0 | exit> }}"
            ))
            .is_ok()
        );
        assert!(
            check(
                "func plus_one(out: i64) <- i64 { mu i64 { n => <(n, 1) | __add | out> } }
                 func double(n: i64) -> i64 { (<(n, 2) | __mul) }
                 proc main | (exit: -i32) / {IO} {
                     <mu i64 { out <= <20 | plus_one | double | out> } | println; <0 | exit> }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_function_is_not_accepted_at_its_other_orientation() {
        // `String <- i64` is not `i64 -> String`.
        assert!(
            check(
                "menu Deliver { deliver: (i64 -> String) }
                 func deliver_i64(out: String) <- i64 { mu i64 { n => <n | int_to_str | out> } }
                 func delivers() -> Deliver { mu Deliver { deliver <= <deliver_i64 | deliver> } }"
            )
            .is_err()
        );
        assert!(
            check(
                "func deliver_i64(out: String) <- i64 { mu i64 { n => <n | int_to_str | out> } }
                 func f() -> i64 { let p: ((i64 -> String), i64) = (deliver_i64, 1); 0 }"
            )
            .is_err()
        );
        assert!(
            check(
                "enum Held<-T> { Holds(T) }
             func deliver_i64(out: String) <- i64 { mu i64 { n => <n | int_to_str | out> } }
             func f() -> i64 { let h: Held<(i64 -> String)> = Held::Holds(deliver_i64); 0 }",
            )
            .is_err()
        );
    }

    #[test]
    fn a_nullary_declaration_is_called_with_the_unit() {
        // No parameters is the empty product, so `(,)` is what flows in.
        assert!(
            check(
                "func answer() -> i64 { 42 }
                 proc main | (exit: -i32) / {IO} { <(,) | answer | println; <0 | exit> }"
            )
            .is_ok()
        );
        // A value flowing into one is not a call: nothing takes it.
        assert!(
            check(
                "func answer() -> i64 { 42 }
                 proc main | (exit: -i32) / {IO} { <1 | answer | println; <0 | exit> }"
            )
            .is_err()
        );
    }

    #[test]
    fn a_bound_on_a_negative_function_is_discharged_by_the_cut() {
        let prelude = "spec Show { func show(self: +Self) -> String; }
             impl Show for i64 { func show(self: +i64) -> String { \"n\" } }
             impl Show for Bool { func show(self: +Bool) -> String { \"b\" } }
             func emit<+T: Show>(out: -String) <- T { fn(x: T) { <x | show | out> } }\n";
        // Nothing the call receives mentions T; the cut fixes it, at two
        // different types in the same declaration.
        assert!(
            check(&format!(
                "{prelude} proc main | (exit: -i32) / {{IO}} {{
                     <mu String {{ s <= <42 | emit | s> }} | println;
                     <mu String {{ s <= <True | emit | s> }} | println;
                     <0 | exit>
                 }}"
            ))
            .is_ok()
        );
        // A type with no impl is still refused.
        let diags = check(&format!(
            "{prelude} proc main | (exit: -i32) / {{IO}} {{
                 <mu String {{ s <= <\"text\" | emit(s)> }} | println; <0 | exit>
             }}"
        ))
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no `impl Show for String`")), "{diags:?}");
    }

    #[test]
    fn a_trait_method_may_consume_self() {
        // A negative method has no `self` parameter: its Self is what it
        // consumes, and the cut says which impl runs.
        let prelude = "spec Deliver { func deliver(out: -String) <- Self; }
             impl Deliver for i64 {
                 func deliver(out: -String) <- i64 { fn(n: +i64) { <\"i\" | out> } }
             }
             impl Deliver for Bool {
                 func deliver(out: -String) <- Bool { fn(b: +Bool) { <\"b\" | out> } }
             }\n";
        assert!(
            check(&format!(
                "{prelude} proc main | (exit: -i32) / {{IO}} {{
                     <mu String {{ s <= <42 | deliver(s)> }} | println;
                     <mu String {{ s <= <True | deliver(s)> }} | println;
                     <0 | exit>
                 }}"
            ))
            .is_ok()
        );
        let diags = check(&format!(
            "{prelude} proc main | (exit: -i32) / {{IO}} {{
                 <mu String {{ s <= <\"text\" | deliver(s)> }} | println; <0 | exit>
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
                "spec Show { func show(self: +Self) -> String; }
                 impl Show for i64 { func show(self: +i64) -> String { \"n\" } }
                 func wrap<+T: Show>(x: T) -> String { let f = fn(y: T) { show(y) }; f(x) }"
            )
            .is_ok()
        );
        assert!(
            check(
                "spec Show { func show(self: +Self) -> String; }
                 impl Show for i64 { func show(self: +i64) -> String { \"n\" } }
                 func annotated<+T: Show>(x: T) -> String { let y: T = x; show(y) }"
            )
            .is_ok()
        );
        // The negative shape's body checks on its own too.
        assert!(
            check(
                "spec Show { func show(self: +Self) -> String; }
                 impl Show for i64 { func show(self: +i64) -> String { \"n\" } }
                 func emit<+T: Show>(out: -String) <- T { fn(x: T) { <show(x) | out> } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_consumer_travels_bare() {
        // A continuation is a value: it passes as an ordinary argument and
        // sits in bindings without any box.
        assert!(check("func hold(k: -i64) -> (;) { <1 | k> }").is_ok());
    }

    #[test]
    fn the_cut_stays_oriented() {
        // A raw consumer is a value everywhere except the left of a cut:
        // there, involution would let any positive pass for a consumer of
        // consumers, and the machine only runs an oriented cut.
        let diags = check("func f(k: -i64, target: -i64) -> (;) { <k | target> }").unwrap_err();
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
                "func dne<+T>(t: -(-T)) -> T { t }
                 proc main | (exit: -i32) / {IO} { <42 | dne | println; <0 | exit> }",
            )
            .is_ok()
        );
    }

    #[test]
    fn value_arguments_are_checked_against_the_declaration() {
        let diags = check(
            "func f(x: +String) -> i64 { 0 }
             proc main | (exit: -i32) / {IO} { <f(42) | println; <0 | exit> }",
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
        // `-(-A)` — the same type, by the involution — be called like a
        // function.
        let diags = check("func f(x: +i64) -> i64 { x(1) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("which is not a function")), "{diags:?}");
    }

    #[test]
    fn a_select_reads_its_type_off_its_arms() {
        // A bare variant name says which enum, so writing it again is
        // redundant.
        assert!(
            check(
                "enum Color { Red, Green }
                 func code(return: -i32) <- Color {
                     mu { Red => <0 | return>, Green => <1 | return> }
                 }"
            )
            .is_ok()
        );

        // Exhaustiveness of an inferred type is checked in `exhaustive`.
    }

    #[test]
    fn a_select_in_a_negative_fn_takes_the_type_it_consumes() {
        // Nothing in `n <= …` names a type, but the declaration already did.
        assert!(
            check("func twice(out: -i64) <- +i64 { mu { n => <(n, 2) | __mul | out> } }").is_ok()
        );
        let diags = check("func twice(out: -String) <- +i64 { mu { n => <str_len(n) | out> } }")
            .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );

        // Outside one, with no arm naming a type, it has to be written.
        let diags = check(
            "proc main | (exit: -i32) / {IO} {
                 let show = mu { n => <n | println };
                 <42 | show>;
                 <0 | exit>
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
            "proc main | (exit: -i32) / {IO} {
                 let complain = mu { m => { <m | println; 1 | exit> } };
                 let text = mu { k <= __read_file(\"in\", k, complain) };
                 <(text, 1) | __add | println;
                 <0 | exit>
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("(+String, +i64)")),
            "the inferred type should reach the use: {diags:?}"
        );

        // A cut says it just as well: `42 | k` makes `k` a consumer of i64.
        let diags = check(
            "proc main | (exit: -i32) / {IO} {
                 let answer = mu { k <= <42 | k> };
                 <str_len(answer) | println;
                 <0 | exit>
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
            check("func show(out: -String) <- +i64 { mu +i64 { n => <int_to_str(n) | out> } }")
                .is_ok()
        );

        let diags =
            check("func show(out: -String) <- +i64 { mu +i64 { n => <str_len(n) | out> } }")
                .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );
    }
}
