//! Trait elaboration.
//!
//! Traits dispatch on the runtime type of a method's first argument, checked
//! total by the type checker. This pass turns each `impl` method into a
//! mangled top-level function and records a registry: the method signatures
//! (for the checker), the bounds (already on declarations), and, per method
//! name, which function implements it for which type key. The driver binds
//! each trait-method name to a dispatching value and hands the impl functions
//! to the runtime from this registry.
//!
//! `Trait` and `Impl` declarations are removed from the program the rest of
//! the pipeline sees; only the mangled implementation functions remain.

use crate::ast::*;
use crate::token::Span;
use slc_core::types::Type;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct TraitError {
    pub message: String,
    pub span: Span,
}

/// A bound of a generic impl, pointing at the type argument it constrains.
/// `impl<+T: Into<String>> Show for Id<T>` records position 0, `Into`, and
/// `String`. The dictionary is built from the type at that position.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedBound {
    pub position: usize,
    pub trait_name: String,
    pub args: Vec<TypeExpr>,
}

/// One impl, as elaboration recorded it. `key` identifies it among the
/// trait's impls: the implementing type when the trait takes no arguments,
/// and those arguments beside it when it does.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplHead {
    pub key: String,
    pub self_key: String,
    pub for_type: TypeExpr,
    pub trait_args: Vec<TypeExpr>,
    pub type_params: Vec<String>,
    pub bounds: Vec<PositionedBound>,
}

/// An impl selected for one use, with its type parameters instantiated.
#[derive(Debug, Clone)]
pub struct ImplMatch {
    pub key: String,
    pub bounds: Vec<PositionedBound>,
    pub subst: HashMap<String, Type>,
}

/// What the checker and runtime need to know about traits.
#[derive(Debug, Default, Clone)]
pub struct TraitInfo {
    /// Trait name → its method signatures.
    pub traits: HashMap<String, Vec<TraitMethod>>,
    /// Trait name → its type parameters, in order (`Into`'s `U`).
    pub trait_params: HashMap<String, Vec<String>>,
    /// Trait name → the polarity each of those parameters declares.
    pub trait_param_signs: HashMap<String, Vec<(String, ParamPolarity)>>,
    /// Trait name → where it is declared, for diagnostics about its
    /// signatures once the declaration itself is gone.
    pub spans: HashMap<String, Span>,
    /// Method name → the trait that declares it (unique across traits in v1).
    pub method_owner: HashMap<String, String>,
    /// Method name → (impl key → the mangled function implementing it).
    pub method_impls: HashMap<String, HashMap<String, String>>,
    /// Trait name → its impls, for selection at a use. A bounded impl's
    /// bounds ride on the head: its dictionary is constructed from them.
    pub heads: HashMap<String, Vec<ImplHead>>,
}

impl TraitInfo {
    /// Is `method` a trait method?
    pub fn is_method(&self, method: &str) -> bool {
        self.method_owner.contains_key(method)
    }
    /// The signature of a trait method.
    pub fn method_sig(&self, method: &str) -> Option<&TraitMethod> {
        let trait_name = self.method_owner.get(method)?;
        self.traits.get(trait_name)?.iter().find(|m| m.name == method)
    }
    /// The type parameters of `trait_name`, or none when it is not a trait.
    pub fn params_of(&self, trait_name: &str) -> &[String] {
        self.trait_params.get(trait_name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The impl of `trait_name` for `self_ty` at `args`. One impl matches, or
    /// the use is refused: none does, or more than one does.
    pub fn select(
        &self,
        trait_name: &str,
        self_ty: &Type,
        args: &[Type],
    ) -> Result<ImplMatch, String> {
        let mut found: Vec<ImplMatch> = Vec::new();
        if let Some(heads) = self.heads.get(trait_name) {
            for head in heads {
                if let Some(subst) = match_head(head, self_ty, args) {
                    found.push(ImplMatch {
                        key: head.key.clone(),
                        bounds: head.bounds.clone(),
                        subst,
                    });
                }
            }
        }
        match found.len() {
            1 => Ok(found.pop().expect("one impl")),
            0 => Err(format!(
                "no `impl {} for {}`",
                applied(trait_name, args),
                type_display(self_ty)
            )),
            _ => Err(format!(
                "`{}` for {} is implemented more than once",
                applied(trait_name, args),
                type_display(self_ty)
            )),
        }
    }
}

/// `Into<i64>`, or the bare trait when it takes no arguments.
fn applied(trait_name: &str, args: &[Type]) -> String {
    if args.is_empty() {
        return trait_name.to_string();
    }
    let args = args.iter().map(type_display).collect::<Vec<_>>().join(", ");
    format!("{trait_name}<{args}>")
}

/// A type the way an impl header writes it: polarity marks dropped, so a
/// value of type `i64` is `i64`.
fn type_display(ty: &Type) -> String {
    match ty {
        Type::Pos(b) | Type::Neg(b) => b.to_string(),
        Type::Named(name, args) => applied(name, args),
        Type::Dual(inner) | Type::Rowed(inner, _) | Type::Delayed(inner, _) => type_display(inner),
        Type::Tensor(items) => {
            format!("({})", items.iter().map(type_display).collect::<Vec<_>>().join(", "))
        }
        other => other.to_string(),
    }
}

/// The runtime-facing type key of a written type: what `impl Show for T`
/// dispatches on. Must agree with the runtime's key for a value.
pub fn type_key(ty: &TypeExpr) -> Option<String> {
    match ty {
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) | TypeExpr::Dual(inner) => {
            type_key(&inner.kind)
        }
        TypeExpr::Base(name) => Some(name.clone()),
        // A generic declaration keys by its name: `impl<+T: …> … for List<T>`
        // covers every instantiation, its bound discharged per element type.
        TypeExpr::Apply(name, _) => Some(name.clone()),
        // An anonymous type keys by its connective and width, and its
        // components stand where a declaration's type arguments do.
        TypeExpr::Tensor(items) => Some(anonymous_key("tuple", items.len())),
        TypeExpr::Sum(items) if !items.is_empty() => Some(anonymous_key("choice", items.len())),
        _ => None,
    }
}

/// The key of an anonymous type: `$unit`, `$tuple3`, `$choice2`. The `$`
/// keeps it apart from every declared name, which cannot begin with one.
pub fn anonymous_key(kind: &str, width: usize) -> String {
    if kind == "tuple" && width == 0 { "$unit".to_string() } else { format!("${kind}{width}") }
}

/// Mangled name of an impl method: opaque, cannot collide with a source name.
fn mangle(trait_name: &str, key: &str, method: &str) -> String {
    format!("{trait_name}#{key}#{method}")
}

/// Elaborate a resolved program: collect the registry, rewrite impls to
/// mangled functions, and drop `trait`/`impl` declarations.
pub fn elaborate(program: &Program) -> Result<(Program, TraitInfo), Vec<TraitError>> {
    let mut info = TraitInfo::default();
    let mut errors = Vec::new();

    // Traits first: names, methods, and method-name uniqueness.
    for d in &program.decls {
        if let Decl::Trait { name, methods, type_params, type_param_signs, .. } = &d.kind {
            info.trait_params.insert(name.clone(), type_params.clone());
            info.trait_param_signs.insert(name.clone(), type_param_signs.clone());
            for m in methods {
                if let Some(other) = info.method_owner.insert(m.name.clone(), name.clone()) {
                    errors.push(TraitError {
                        message: format!(
                            "method `{}` is declared by both `{other}` and `{name}`; method \
                             names are unique across traits",
                            m.name
                        ),
                        span: d.span,
                    });
                }
            }
            info.traits.insert(name.clone(), methods.clone());
            info.spans.insert(name.clone(), d.span);
        }
    }

    // Impls: coherence, method rewriting, registry.
    let mut out = Vec::new();
    for d in &program.decls {
        match &d.kind {
            Decl::Trait { .. } => {}
            Decl::Impl {
                trait_name,
                trait_args,
                type_params,
                type_param_signs,
                bounds,
                for_type,
                methods,
            } => {
                let Some(self_key) = type_key(for_type) else {
                    errors.push(TraitError {
                        message: "this type cannot carry an impl in v1".into(),
                        span: d.span,
                    });
                    continue;
                };
                let Some(declared) = info.trait_params.get(trait_name).cloned() else {
                    errors.push(TraitError {
                        message: format!("`impl` of unknown trait `{trait_name}`"),
                        span: d.span,
                    });
                    continue;
                };
                if trait_args.len() != declared.len() {
                    errors.push(TraitError {
                        message: format!(
                            "`{trait_name}` takes {} type argument{}, and this impl supplies {}",
                            declared.len(),
                            if declared.len() == 1 { "" } else { "s" },
                            trait_args.len()
                        ),
                        span: d.span,
                    });
                    continue;
                }
                let key = impl_key(&self_key, trait_args);
                if let Some(earlier) = info.heads.get(trait_name).and_then(|heads| {
                    heads.iter().find(|head| {
                        heads_overlap(
                            &head.type_params,
                            &head.for_type,
                            &head.trait_args,
                            type_params,
                            for_type,
                            trait_args,
                        )
                    })
                }) {
                    errors.push(TraitError {
                        message: format!(
                            "`impl {} for {}` overlaps `impl {} for {}`; one impl per trait, its \
                             arguments, and the implementing type",
                            applied_expr(trait_name, trait_args),
                            type_expr_display(for_type),
                            applied_expr(trait_name, &earlier.trait_args),
                            type_expr_display(&earlier.for_type)
                        ),
                        span: d.span,
                    });
                    continue;
                }
                // `impl<+T: Show> Display for List<T>`: each bound points at
                // the position its parameter holds in the for-type's
                // arguments, so a call can read the element type off the
                // receiver.
                let positioned_bounds = position_bounds(for_type, bounds);
                let head = ImplHead {
                    key: key.clone(),
                    self_key,
                    for_type: for_type.clone(),
                    trait_args: trait_args.clone(),
                    type_params: type_params.clone(),
                    bounds: positioned_bounds.clone(),
                };
                let trait_methods = info.traits.get(trait_name).cloned().unwrap_or_default();
                let subst = trait_subst(for_type, &declared, trait_args);
                let mut seen_methods = HashSet::new();
                for method in methods {
                    let method_name = decl_name(&method.kind);
                    let Some(signature) = trait_methods.iter().find(|m| m.name == method_name)
                    else {
                        errors.push(TraitError {
                            message: format!("`{trait_name}` has no method `{method_name}`"),
                            span: method.span,
                        });
                        continue;
                    };
                    if let Some(message) = method_mismatch(signature, &method.kind, &subst) {
                        errors.push(TraitError { message, span: method.span });
                    }
                    seen_methods.insert(method_name.clone());
                    let mangled = mangle(trait_name, &key, &method_name);
                    info.method_impls
                        .entry(method_name)
                        .or_default()
                        .insert(key.clone(), mangled.clone());
                    out.push(Node {
                        span: method.span,
                        kind: rename_decl(
                            &method.kind,
                            &mangled,
                            type_params,
                            type_param_signs,
                            bounds,
                        ),
                    });
                }
                for signature in &trait_methods {
                    if !seen_methods.contains(&signature.name) {
                        errors.push(TraitError {
                            message: format!(
                                "`impl {} for {}` does not implement `{}`",
                                applied_expr(trait_name, trait_args),
                                type_expr_display(for_type),
                                signature.name
                            ),
                            span: d.span,
                        });
                    }
                }
                info.heads.entry(trait_name.clone()).or_default().push(head);
            }
            _ => out.push(d.clone()),
        }
    }

    if errors.is_empty() { Ok((Program { decls: out }, info)) } else { Err(errors) }
}

fn decl_name(d: &Decl) -> String {
    match d {
        Decl::Fn { name, .. } | Decl::Command { name, .. } => name.clone(),
        _ => String::new(),
    }
}

fn rename_decl(
    d: &Decl,
    new_name: &str,
    impl_params: &[String],
    impl_signs: &[(String, ParamPolarity)],
    impl_bounds: &[TraitBound],
) -> Decl {
    let mut d = d.clone();
    match &mut d {
        Decl::Fn { name, type_params, type_param_signs, bounds, .. } => {
            *name = new_name.to_string();
            // The impl's `<+T: Show>` becomes the method's, so its body checks
            // generically with `T` rigid, its polarity and bound in scope.
            prepend(type_params, impl_params);
            prepend_signs(type_param_signs, impl_signs);
            prepend_bounds(bounds, impl_bounds);
        }
        Decl::Command { name, type_params, type_param_signs, bounds, .. } => {
            *name = new_name.to_string();
            prepend(type_params, impl_params);
            prepend_signs(type_param_signs, impl_signs);
            prepend_bounds(bounds, impl_bounds);
        }
        _ => {}
    }
    d
}

fn prepend_signs(into: &mut Vec<(String, ParamPolarity)>, extra: &[(String, ParamPolarity)]) {
    for sign in extra {
        if !into.iter().any(|(name, _)| name == &sign.0) {
            into.push(sign.clone());
        }
    }
}

fn prepend(into: &mut Vec<String>, extra: &[String]) {
    for (i, p) in extra.iter().enumerate() {
        if !into.contains(p) {
            into.insert(i, p.clone());
        }
    }
}

fn prepend_bounds(into: &mut Vec<TraitBound>, extra: &[TraitBound]) {
    for b in extra {
        if !into.contains(b) {
            into.push(b.clone());
        }
    }
}

/// The impl's identity. No trait arguments keeps the implementing type's key,
/// which is what every impl had before a trait could take parameters.
fn impl_key(self_key: &str, trait_args: &[TypeExpr]) -> String {
    let args = rendered_args(trait_args);
    if args.is_empty() { self_key.to_string() } else { format!("{args}#{self_key}") }
}

/// The dictionary-name fragment of a trait's arguments, empty when it has none.
pub fn rendered_args(args: &[TypeExpr]) -> String {
    args.iter().map(type_expr_key).collect::<Vec<_>>().join(",")
}

fn type_expr_key(ty: &TypeExpr) -> String {
    match strip_mark(ty) {
        TypeExpr::Base(name) => name.clone(),
        TypeExpr::Apply(name, args) => {
            let args =
                args.iter().map(|arg| type_expr_key(&arg.kind)).collect::<Vec<_>>().join(",");
            format!("{name}<{args}>")
        }
        TypeExpr::Tensor(items) => {
            let items =
                items.iter().map(|item| type_expr_key(&item.kind)).collect::<Vec<_>>().join(",");
            format!("({items})")
        }
        other => type_expr_display(other),
    }
}

fn type_expr_display(ty: &TypeExpr) -> String {
    match strip_mark(ty) {
        TypeExpr::Base(name) => name.clone(),
        TypeExpr::Apply(name, args) => {
            applied_expr(name, &args.iter().map(|a| a.kind.clone()).collect::<Vec<_>>())
        }
        other => format!("{other:?}"),
    }
}

fn applied_expr(trait_name: &str, args: &[TypeExpr]) -> String {
    if args.is_empty() {
        return trait_name.to_string();
    }
    let args = args.iter().map(type_expr_key).collect::<Vec<_>>().join(", ");
    format!("{trait_name}<{args}>")
}

fn position_bounds(for_type: &TypeExpr, bounds: &[TraitBound]) -> Vec<PositionedBound> {
    let args = match for_type {
        TypeExpr::Apply(_, args) | TypeExpr::Tensor(args) | TypeExpr::Sum(args) => args,
        _ => return Vec::new(),
    };
    bounds
        .iter()
        .filter_map(|bound| {
            args.iter()
                .position(|arg| matches!(&arg.kind, TypeExpr::Base(name) if name == &bound.param))
                .map(|position| PositionedBound {
                    position,
                    trait_name: bound.trait_name.clone(),
                    args: bound.args.clone(),
                })
        })
        .collect()
}

/// `Self` and the trait's parameters, substituted from the impl header.
fn trait_subst<'a>(
    for_type: &'a TypeExpr,
    params: &'a [String],
    args: &'a [TypeExpr],
) -> HashMap<&'a str, &'a TypeExpr> {
    let mut subst = HashMap::from([("Self", for_type)]);
    for (param, arg) in params.iter().zip(args) {
        subst.insert(param.as_str(), arg);
    }
    subst
}

/// Where the impl's method does not have the trait's signature with `Self`
/// and the trait arguments substituted.
fn method_mismatch(
    signature: &TraitMethod,
    method: &Decl,
    subst: &HashMap<&str, &TypeExpr>,
) -> Option<String> {
    let (is_command, polarity, values, continuations, result) = match method {
        Decl::Fn { polarity, params, return_type, .. } => {
            (false, *polarity, params.as_slice(), &[][..], return_type.as_ref())
        }
        Decl::Command { value_params, continuation_params, .. } => (
            true,
            FunctionPolarity::Positive,
            value_params.as_slice(),
            continuation_params.as_slice(),
            None,
        ),
        _ => return Some("a trait method is implemented by a `fn` or a `command`".into()),
    };
    if is_command != signature.is_command {
        return Some(format!(
            "`{}` is a {}, and this impl writes a {}",
            signature.name,
            if signature.is_command { "command" } else { "function" },
            if is_command { "command" } else { "function" }
        ));
    }
    if !is_command && polarity != signature.polarity {
        return Some(format!("`{}` faces the other way from its declaration", signature.name));
    }
    if values.len() != signature.value_params.len()
        || continuations.len() != signature.continuation_params.len()
    {
        return Some(format!(
            "`{}` takes {} value parameter{} and {} continuation{}, and this impl writes \
             another group",
            signature.name,
            signature.value_params.len(),
            if signature.value_params.len() == 1 { "" } else { "s" },
            signature.continuation_params.len(),
            if signature.continuation_params.len() == 1 { "" } else { "s" }
        ));
    }
    for (expected, written) in signature
        .value_params
        .iter()
        .chain(signature.continuation_params.iter())
        .zip(values.iter().chain(continuations.iter()))
    {
        match (&expected.ty, &written.ty) {
            (Some(expected), Some(written)) if type_eq(&substitute(expected, subst), written) => {}
            _ => {
                return Some(format!(
                    "`{}` takes {}, and this impl takes {}",
                    signature.name,
                    expected
                        .ty
                        .as_ref()
                        .map(|ty| type_expr_key(&substitute(ty, subst)))
                        .unwrap_or_else(|| "_".into()),
                    written.ty.as_ref().map(type_expr_key).unwrap_or_else(|| "_".into())
                ));
            }
        }
    }
    match (&signature.return_type, result) {
        (Some(expected), Some(written)) if type_eq(&substitute(expected, subst), written) => None,
        (None, None) => None,
        (Some(expected), written) => Some(format!(
            "`{}` returns {}, and this impl returns {}",
            signature.name,
            type_expr_key(&substitute(expected, subst)),
            written.map(type_expr_key).unwrap_or_else(|| "(;)".into())
        )),
        (None, Some(written)) => Some(format!(
            "`{}` returns no value, and this impl returns {}",
            signature.name,
            type_expr_key(written)
        )),
    }
}

fn substitute(ty: &TypeExpr, subst: &HashMap<&str, &TypeExpr>) -> TypeExpr {
    match ty {
        TypeExpr::Base(name) => {
            subst.get(name.as_str()).copied().cloned().unwrap_or_else(|| ty.clone())
        }
        TypeExpr::Apply(name, args) => TypeExpr::Apply(
            name.clone(),
            args.iter()
                .map(|arg| Node { span: arg.span, kind: substitute(&arg.kind, subst) })
                .collect(),
        ),
        TypeExpr::Positive(inner) => TypeExpr::Positive(Box::new(Node {
            span: inner.span,
            kind: substitute(&inner.kind, subst),
        })),
        TypeExpr::Negative(inner) => TypeExpr::Negative(Box::new(Node {
            span: inner.span,
            kind: substitute(&inner.kind, subst),
        })),
        TypeExpr::Dual(inner) => TypeExpr::Dual(Box::new(Node {
            span: inner.span,
            kind: substitute(&inner.kind, subst),
        })),
        TypeExpr::Tensor(items) => TypeExpr::Tensor(subst_nodes(items, subst)),
        TypeExpr::Par(items) => TypeExpr::Par(subst_nodes(items, subst)),
        TypeExpr::With(items) => TypeExpr::With(subst_nodes(items, subst)),
        TypeExpr::Sum(items) => TypeExpr::Sum(subst_nodes(items, subst)),
        TypeExpr::Fun(a, b) => TypeExpr::Fun(
            Box::new(Node { span: a.span, kind: substitute(&a.kind, subst) }),
            Box::new(Node { span: b.span, kind: substitute(&b.kind, subst) }),
        ),
        TypeExpr::Effectful(inner, row) => TypeExpr::Effectful(
            Box::new(Node { span: inner.span, kind: substitute(&inner.kind, subst) }),
            row.clone(),
        ),
        TypeExpr::Row(_) => ty.clone(),
    }
}

fn subst_nodes(items: &[Node<TypeExpr>], subst: &HashMap<&str, &TypeExpr>) -> Vec<Node<TypeExpr>> {
    items.iter().map(|item| Node { span: item.span, kind: substitute(&item.kind, subst) }).collect()
}

fn strip_mark(ty: &TypeExpr) -> &TypeExpr {
    match ty {
        TypeExpr::Positive(inner) | TypeExpr::Negative(inner) => strip_mark(&inner.kind),
        _ => ty,
    }
}

fn type_eq(a: &TypeExpr, b: &TypeExpr) -> bool {
    match (strip_mark(a), strip_mark(b)) {
        (TypeExpr::Base(x), TypeExpr::Base(y)) => x == y,
        (TypeExpr::Apply(x, xs), TypeExpr::Apply(y, ys)) => {
            x == y
                && xs.len() == ys.len()
                && xs.iter().zip(ys).all(|(p, q)| type_eq(&p.kind, &q.kind))
        }
        (TypeExpr::Tensor(xs), TypeExpr::Tensor(ys))
        | (TypeExpr::Par(xs), TypeExpr::Par(ys))
        | (TypeExpr::With(xs), TypeExpr::With(ys))
        | (TypeExpr::Sum(xs), TypeExpr::Sum(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(p, q)| type_eq(&p.kind, &q.kind))
        }
        (TypeExpr::Fun(a1, b1), TypeExpr::Fun(a2, b2)) => {
            type_eq(&a1.kind, &a2.kind) && type_eq(&b1.kind, &b2.kind)
        }
        (TypeExpr::Dual(x), TypeExpr::Dual(y)) => type_eq(&x.kind, &y.kind),
        _ => false,
    }
}

/// Two impls overlap when some instantiation makes their implementing types
/// and their trait arguments the same. A type parameter stands for any type.
fn heads_overlap(
    a_params: &[String],
    a_for: &TypeExpr,
    a_args: &[TypeExpr],
    b_params: &[String],
    b_for: &TypeExpr,
    b_args: &[TypeExpr],
) -> bool {
    if a_args.len() != b_args.len() {
        return false;
    }
    let mut subst = HashMap::new();
    unify_expr(a_for, b_for, a_params, b_params, &mut subst)
        && a_args
            .iter()
            .zip(b_args)
            .all(|(left, right)| unify_expr(left, right, a_params, b_params, &mut subst))
}

#[derive(Clone, Debug)]
enum Pat {
    Var(String),
    Base(String),
    Apply(String, Vec<Pat>),
    Tuple(Vec<Pat>),
    Other,
}

fn to_pat(ty: &TypeExpr, params: &[String], side: &str) -> Pat {
    match strip_mark(ty) {
        TypeExpr::Base(name) if params.iter().any(|param| param == name) => {
            Pat::Var(format!("{side}:{name}"))
        }
        TypeExpr::Base(name) => Pat::Base(name.clone()),
        TypeExpr::Apply(name, args) => Pat::Apply(
            name.clone(),
            args.iter().map(|arg| to_pat(&arg.kind, params, side)).collect(),
        ),
        TypeExpr::Tensor(items) => {
            Pat::Tuple(items.iter().map(|item| to_pat(&item.kind, params, side)).collect())
        }
        _ => Pat::Other,
    }
}

fn unify_expr(
    left: &TypeExpr,
    right: &TypeExpr,
    left_params: &[String],
    right_params: &[String],
    subst: &mut HashMap<String, Pat>,
) -> bool {
    unify_pat(&to_pat(left, left_params, "L"), &to_pat(right, right_params, "R"), subst)
}

fn walk(pat: &Pat, subst: &HashMap<String, Pat>) -> Pat {
    match pat {
        Pat::Var(name) => match subst.get(name) {
            Some(bound) => walk(bound, subst),
            None => pat.clone(),
        },
        Pat::Apply(name, args) => {
            Pat::Apply(name.clone(), args.iter().map(|arg| walk(arg, subst)).collect())
        }
        Pat::Tuple(items) => Pat::Tuple(items.iter().map(|item| walk(item, subst)).collect()),
        other => other.clone(),
    }
}

fn occurs(var: &str, pat: &Pat) -> bool {
    match pat {
        Pat::Var(name) => name == var,
        Pat::Apply(_, args) | Pat::Tuple(args) => args.iter().any(|arg| occurs(var, arg)),
        _ => false,
    }
}

fn unify_pat(left: &Pat, right: &Pat, subst: &mut HashMap<String, Pat>) -> bool {
    let left = walk(left, subst);
    let right = walk(right, subst);
    match (&left, &right) {
        (Pat::Var(name), Pat::Var(other)) if name == other => true,
        (Pat::Var(name), _) => {
            if occurs(name, &right) {
                return false;
            }
            subst.insert(name.clone(), right);
            true
        }
        (_, Pat::Var(name)) => {
            if occurs(name, &left) {
                return false;
            }
            subst.insert(name.clone(), left);
            true
        }
        (Pat::Base(x), Pat::Base(y)) => x == y,
        (Pat::Apply(x, xs), Pat::Apply(y, ys)) => {
            x == y && xs.len() == ys.len() && xs.iter().zip(ys).all(|(p, q)| unify_pat(p, q, subst))
        }
        (Pat::Tuple(xs), Pat::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(p, q)| unify_pat(p, q, subst))
        }
        (Pat::Other, Pat::Other) => false,
        _ => false,
    }
}

/// Does this impl cover `self_ty` at `args`? The substitution binds every
/// type parameter the impl declares.
fn match_head(head: &ImplHead, self_ty: &Type, args: &[Type]) -> Option<HashMap<String, Type>> {
    if head.trait_args.len() != args.len() {
        return None;
    }
    let params: HashSet<&str> = head.type_params.iter().map(String::as_str).collect();
    let mut subst = HashMap::new();
    if !match_type(&head.for_type, self_ty, &params, &mut subst) {
        return None;
    }
    for (pattern, actual) in head.trait_args.iter().zip(args) {
        if !match_type(pattern, actual, &params, &mut subst) {
            return None;
        }
    }
    if head.type_params.iter().any(|param| !subst.contains_key(param)) {
        return None;
    }
    Some(subst)
}

fn match_type(
    pattern: &TypeExpr,
    actual: &Type,
    params: &HashSet<&str>,
    subst: &mut HashMap<String, Type>,
) -> bool {
    let actual = peel(actual);
    match strip_mark(pattern) {
        TypeExpr::Base(name) if params.contains(name.as_str()) => {
            if let Some(existing) = subst.get(name) {
                existing == actual
            } else {
                subst.insert(name.clone(), actual.clone());
                true
            }
        }
        TypeExpr::Base(name) => same_atom(actual, name),
        TypeExpr::Apply(name, args) => {
            let Type::Named(actual_name, actual_args) = actual else { return false };
            // `Stream<T>` covers `Stream<i64, row>`: a row argument the impl
            // header left out is filled in at the use, and is not part of
            // the pattern.
            if actual_name != name || args.len() > actual_args.len() {
                return false;
            }
            args.iter()
                .zip(actual_args)
                .all(|(pattern, actual)| match_type(&pattern.kind, actual, params, subst))
        }
        TypeExpr::Tensor(items) => {
            let Type::Tensor(actual_items) = actual else { return false };
            items.len() == actual_items.len()
                && items
                    .iter()
                    .zip(actual_items)
                    .all(|(pattern, actual)| match_type(&pattern.kind, actual, params, subst))
        }
        TypeExpr::Sum(items) => {
            let Type::Sum(actual_items) = actual else { return false };
            items.len() == actual_items.len()
                && items
                    .iter()
                    .zip(actual_items)
                    .all(|(pattern, actual)| match_type(&pattern.kind, actual, params, subst))
        }
        _ => false,
    }
}

fn peel(ty: &Type) -> &Type {
    match ty {
        Type::Dual(inner) | Type::Rowed(inner, _) | Type::Delayed(inner, _) => peel(inner),
        _ => ty,
    }
}

fn same_atom(ty: &Type, name: &str) -> bool {
    match peel(ty) {
        Type::Pos(base) | Type::Neg(base) => base.to_string() == name,
        Type::Named(actual, args) if args.is_empty() => actual == name,
        _ => false,
    }
}
