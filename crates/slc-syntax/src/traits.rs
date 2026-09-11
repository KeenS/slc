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
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct TraitError {
    pub message: String,
    pub span: Span,
}

/// What the checker and runtime need to know about traits.
#[derive(Debug, Default, Clone)]
pub struct TraitInfo {
    /// Trait name → its method signatures.
    pub traits: HashMap<String, Vec<TraitMethod>>,
    /// Method name → the trait that declares it (unique across traits in v1).
    pub method_owner: HashMap<String, String>,
    /// Method name → (type key → the mangled function implementing it).
    pub method_impls: HashMap<String, HashMap<String, String>>,
    /// Every (trait, type key) with an impl — for bound checking.
    pub impls: std::collections::HashSet<(String, String)>,
    /// (trait, type key) → the impl's own bounds, as (position of the
    /// bound parameter in the impl's `for_type` arguments, trait). A
    /// dictionary for such an impl is *constructed*: the global applied to
    /// one dictionary per entry, read off the use's type arguments.
    pub impl_bounds: HashMap<(String, String), Vec<(usize, String)>>,
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
    /// Does `typekey` have an impl of `trait_name`?
    pub fn has_impl(&self, trait_name: &str, typekey: &str) -> bool {
        self.impls.contains(&(trait_name.to_string(), typekey.to_string()))
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
        // A generic declaration keys by its name: `impl<T: …> … for List<T>`
        // covers every instantiation, its bound discharged per element type.
        TypeExpr::Apply(name, _) => Some(name.clone()),
        _ => None,
    }
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
        if let Decl::Trait { name, methods } = &d.kind {
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
        }
    }

    // Impls: coherence, method rewriting, registry.
    let mut out = Vec::new();
    for d in &program.decls {
        match &d.kind {
            Decl::Trait { .. } => {}
            Decl::Impl { trait_name, type_params, bounds, for_type, methods } => {
                let Some(key) = type_key(for_type) else {
                    errors.push(TraitError {
                        message: "this type cannot carry an impl in v1".into(),
                        span: d.span,
                    });
                    continue;
                };
                if !info.traits.contains_key(trait_name) {
                    errors.push(TraitError {
                        message: format!("`impl` of unknown trait `{trait_name}`"),
                        span: d.span,
                    });
                    continue;
                }
                if !info.impls.insert((trait_name.clone(), key.clone())) {
                    errors.push(TraitError {
                        message: format!(
                            "a second `impl {trait_name} for {key}`; one impl per trait and type"
                        ),
                        span: d.span,
                    });
                    continue;
                }
                // `impl<T: Show> Display for List<T>`: each bound points at
                // the position its parameter holds in the for-type's
                // arguments, so a call can read the element type off the
                // receiver.
                let positioned_bounds: Vec<(usize, String)> = match &for_type {
                    TypeExpr::Apply(_, args) => bounds
                        .iter()
                        .filter_map(|(param, tr)| {
                            args.iter()
                                .position(|a| matches!(&a.kind, TypeExpr::Base(n) if n == param))
                                .map(|i| (i, tr.clone()))
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if !positioned_bounds.is_empty() {
                    info.impl_bounds.insert((trait_name.clone(), key.clone()), positioned_bounds);
                }
                for method in methods {
                    let method_name = decl_name(&method.kind);
                    let mangled = mangle(trait_name, &key, &method_name);
                    info.method_impls
                        .entry(method_name.clone())
                        .or_default()
                        .insert(key.clone(), mangled.clone());
                    out.push(Node {
                        span: method.span,
                        kind: rename_decl(&method.kind, &mangled, type_params, bounds),
                    });
                }
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
    impl_bounds: &[(String, String)],
) -> Decl {
    let mut d = d.clone();
    match &mut d {
        Decl::Fn { name, type_params, bounds, .. } => {
            *name = new_name.to_string();
            // The impl's `<T: Show>` becomes the method's, so its body checks
            // generically with `T` rigid and its bound in scope.
            prepend(type_params, impl_params);
            prepend_bounds(bounds, impl_bounds);
        }
        Decl::Command { name, type_params, bounds, .. } => {
            *name = new_name.to_string();
            prepend(type_params, impl_params);
            prepend_bounds(bounds, impl_bounds);
        }
        _ => {}
    }
    d
}

fn prepend(into: &mut Vec<String>, extra: &[String]) {
    for (i, p) in extra.iter().enumerate() {
        if !into.contains(p) {
            into.insert(i, p.clone());
        }
    }
}

fn prepend_bounds(into: &mut Vec<(String, String)>, extra: &[(String, String)]) {
    for b in extra {
        if !into.contains(b) {
            into.push(b.clone());
        }
    }
}
