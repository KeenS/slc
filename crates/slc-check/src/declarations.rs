//! What the program declares: the variants of each `enum`, the payload each
//! variant carries, the fields of each `data` — and how a written type
//! resolves against them.

use slc_core::types::Type;
use slc_syntax::ast::{Decl, Program, TypeExpr};
use slc_syntax::lower::lower_type;
use std::collections::HashMap;

/// What the checker knows about the program's type declarations: the variants
/// of each `enum`, the payload each variant carries, and the fields of each
/// `data`.
#[derive(Debug, Default)]
pub struct Declarations {
    /// Every declaration name in the program: `data` and `enum` alike.
    pub(crate) declarations: std::collections::HashSet<String>,
    /// Declaration name → variant names, in declaration order.
    variants: HashMap<String, Vec<String>>,
    /// Fully qualified label → declaration name and payload types.
    signatures: HashMap<String, (String, Vec<Type>)>,
    /// Unqualified variant name → its label, when only one enum declares it.
    unqualified: HashMap<String, Option<String>>,
    /// Unqualified destructor name → its label, when only one menu declares
    /// it. Demands are always written `.item(k)`, so they live in their own
    /// namespace: a menu item never shadows a function or a variant.
    destructors: HashMap<String, Option<String>>,
    /// Declaration name → fields, in declaration order.
    pub(crate) records: HashMap<String, Vec<(String, Type)>>,
    /// Declared `menu` names. A menu's items live in `variants` and
    /// `signatures` like an enum's variants — the request view — with each
    /// item's payload being the consumer of its answer.
    pub(crate) menus: std::collections::HashSet<String>,
    /// Declared `form` names. A form's fields live in `records` like a
    /// `data`'s — the demand view — and the form itself is their dual.
    pub(crate) forms: std::collections::HashSet<String>,
    /// Declaration name → its type-parameter count.
    arities: HashMap<String, usize>,
}

impl Declarations {
    /// Lower a written type, resolving a declaration name to its named type.
    /// `lower_type` only knows the built-in types, so `data` and `enum`
    /// names have to be resolved here — including under a sign or a
    /// connective, so `-ParseResult` is a consumer of a declared type.
    pub(crate) fn resolve(&self, ty: &TypeExpr) -> Option<Type> {
        self.resolve_in(ty, &HashMap::new())
    }

    /// Resolve a written type inside a declaration with type parameters:
    /// `params` maps each parameter name to its position, and a bare
    /// parameter resolves to `Type::Param`.
    pub(crate) fn resolve_in(
        &self,
        ty: &TypeExpr,
        params: &HashMap<String, usize>,
    ) -> Option<Type> {
        let resolve = |inner: &TypeExpr| self.resolve_in(inner, params);
        let resolved = match ty {
            TypeExpr::Base(name) if params.contains_key(name) => Type::Param(params[name]),
            // A menu or a form name denotes the negative type itself; its
            // dual — the bare `Named` — is the positive type of its demands.
            TypeExpr::Base(name) if self.is_negative_decl(name) => {
                Type::Dual(Box::new(Type::Named(name.clone(), Vec::new())))
            }
            TypeExpr::Base(name) if self.declarations.contains(name) => {
                Type::Named(name.clone(), Vec::new())
            }
            // A declaration applied to arguments; the argument count must
            // match the declaration's.
            TypeExpr::Apply(name, args) => {
                if self.arities.get(name) != Some(&args.len()) {
                    return None;
                }
                let args = args.iter().map(|a| resolve(&a.kind)).collect::<Option<Vec<_>>>()?;
                if self.is_negative_decl(name) {
                    Type::Dual(Box::new(Type::Named(name.clone(), args)))
                } else {
                    Type::Named(name.clone(), args)
                }
            }
            TypeExpr::Positive(inner) => resolve(&inner.kind)?,
            TypeExpr::Negative(inner) if !inner.kind.is_bottom() => resolve(&inner.kind)?.dual(),
            TypeExpr::Tensor(items) => {
                Type::Tensor(items.iter().map(|item| resolve(&item.kind)).collect::<Option<_>>()?)
            }
            TypeExpr::Par(items) => {
                Type::Par(items.iter().map(|item| resolve(&item.kind)).collect::<Option<_>>()?)
            }
            TypeExpr::With(items) => {
                Type::With(items.iter().map(|item| resolve(&item.kind)).collect::<Option<_>>()?)
            }
            TypeExpr::Sum(items) => {
                Type::Sum(items.iter().map(|item| resolve(&item.kind)).collect::<Option<_>>()?)
            }
            // `A → B` is `-A ⅋ B`.
            TypeExpr::Fun(a, b) => Type::arrow(resolve(&a.kind)?, resolve(&b.kind)?),
            // `dual(A)` applies the involution; only a declaration's name
            // stays wrapped, because it is opaque to the core.
            TypeExpr::Dual(inner) => resolve(&inner.kind)?.dual(),
            // The effect row is the effect checker's concern; the type is
            // the arrow underneath.
            TypeExpr::Effectful(inner, _) => resolve(&inner.kind)?,
            other => return lower_type(other).ok(),
        };
        Some(resolved)
    }

    /// The variant names of a declared enum, in declaration order.
    pub(crate) fn variants_of(&self, name: &str) -> Option<&Vec<String>> {
        self.variants.get(name)
    }

    /// Every enum, with its variant names.
    pub(crate) fn enums(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.variants.iter()
    }

    /// The payload arity of a variant path or unambiguous variant name.
    pub(crate) fn payload_arity(&self, name: &str) -> Option<usize> {
        if let Some((_, payload)) = self.signatures.get(name) {
            return Some(payload.len());
        }
        let label = self.unqualified.get(name)?.as_ref()?;
        self.signatures.get(label).map(|(_, payload)| payload.len())
    }

    /// The field types of a declared struct, in declaration order.
    pub(crate) fn fields(&self, name: &str) -> Option<Vec<Type>> {
        Some(self.records.get(name)?.iter().map(|(_, ty)| ty.clone()).collect())
    }

    /// Resolve a variant path or an unambiguous unqualified variant name.
    /// Whether a name is a declared `data` or `enum`.
    pub(crate) fn declares(&self, name: &str) -> bool {
        self.declarations.contains(name)
    }

    /// A declaration's type-parameter count (0 when unknown).
    pub(crate) fn arity(&self, name: &str) -> usize {
        self.arities.get(name).copied().unwrap_or(0)
    }

    /// Whether a bare name is a variant of more than one enum — in which
    /// case it resolves to nothing, and treating it as a binder would
    /// silently catch everything.
    pub(crate) fn is_ambiguous_variant(&self, name: &str) -> bool {
        matches!(self.unqualified.get(name), Some(None))
    }

    /// Whether a name is a declared `menu`.
    pub(crate) fn is_menu(&self, name: &str) -> bool {
        self.menus.contains(name)
    }

    /// Whether a name is a declared `form`.
    pub(crate) fn is_form(&self, name: &str) -> bool {
        self.forms.contains(name)
    }

    /// Whether a name is a negative declaration — a `menu` or a `form`.
    /// Their values are negative but nominal, so the box discipline that
    /// keeps `-A` out of data positions does not apply to them.
    pub(crate) fn is_negative_decl(&self, name: &str) -> bool {
        self.menus.contains(name) || self.forms.contains(name)
    }

    /// Whether a type is a declared negative type: a menu or form value.
    pub(crate) fn is_negative_value(&self, ty: &Type) -> bool {
        matches!(ty, Type::Dual(inner)
            if matches!(inner.as_ref(), Type::Named(n, _) if self.is_negative_decl(n)))
    }

    /// Resolve a destructor path, or an unambiguous unqualified destructor
    /// name, to the menu it belongs to and the continuation its request
    /// carries.
    pub(crate) fn destructor(&self, name: &str) -> Option<&(String, Vec<Type>)> {
        if let Some(signature) = self.signatures.get(name) {
            return Some(signature);
        }
        let label = self.destructors.get(name)?.as_ref()?;
        self.signatures.get(label)
    }

    /// The menu an item's answer is, if it is one — what a nested
    /// copattern `.item(.inner(k))` refines into.
    pub(crate) fn nested_menu(&self, label: &str) -> Option<&str> {
        match self.destructor(label)?.1.first()? {
            Type::Named(inner, _) if self.is_menu(inner) => Some(inner),
            _ => None,
        }
    }

    pub(crate) fn variant(&self, name: &str) -> Option<&(String, Vec<Type>)> {
        if let Some(signature) = self.signatures.get(name) {
            return Some(signature);
        }
        let label = self.unqualified.get(name)?.as_ref()?;
        self.signatures.get(label)
    }
}

pub(crate) fn enum_types(p: &Program) -> Declarations {
    let mut enums = Declarations::default();
    for d in &p.decls {
        if let Decl::Data { name, .. }
        | Decl::Enum { name, .. }
        | Decl::Menu { name, .. }
        | Decl::Form { name, .. } = &d.kind
        {
            enums.declarations.insert(name.clone());
        }
        if let Decl::Menu { name, .. } = &d.kind {
            enums.menus.insert(name.clone());
        }
        if let Decl::Form { name, .. } = &d.kind {
            enums.forms.insert(name.clone());
        }
        if let Decl::Data { name, type_params, .. }
        | Decl::Enum { name, type_params, .. }
        | Decl::Menu { name, type_params, .. }
        | Decl::Form { name, type_params, .. } = &d.kind
        {
            enums.arities.insert(name.clone(), type_params.len());
        }
    }
    /// A declaration's parameter scope: each name to its position.
    fn param_scope(type_params: &[String]) -> HashMap<String, usize> {
        type_params.iter().enumerate().map(|(i, p)| (p.clone(), i)).collect()
    }
    for d in &p.decls {
        // A form's fields are its demand's fields: the record that feeds
        // it. Field types resolve here, in the second pass, so they may
        // name any declaration — and any of the declaration's own
        // parameters.
        if let Decl::Data { name, type_params, fields, .. }
        | Decl::Form { name, type_params, fields, .. } = &d.kind
        {
            let params = param_scope(type_params);
            enums.records.insert(
                name.clone(),
                fields
                    .iter()
                    .map(|(field, ty)| {
                        (field.clone(), enums.resolve_in(ty, &params).unwrap_or(Type::ONE))
                    })
                    .collect(),
            );
        }
        // A menu registers the request view of itself: one "variant" per
        // item, labelled `Menu::item`, whose payload is the consumer of the
        // item's answer — the continuation a request carries.
        if let Decl::Menu { name, type_params, items, .. } = &d.kind {
            let params = param_scope(type_params);
            enums.variants.insert(name.clone(), items.iter().map(|(i, _)| i.clone()).collect());
            for (item, answer) in items {
                let label = format!("{name}::{item}");
                let answer = enums.resolve_in(answer, &params).unwrap_or(Type::ONE);
                enums.signatures.insert(label.clone(), (name.clone(), vec![answer.dual()]));
                enums
                    .destructors
                    .entry(item.clone())
                    .and_modify(|existing| *existing = None)
                    .or_insert(Some(label));
            }
            continue;
        }
        let Decl::Enum { name, type_params, variants, .. } = &d.kind else { continue };
        let params = param_scope(type_params);
        enums.variants.insert(name.clone(), variants.iter().map(|(v, _)| v.clone()).collect());
        for (variant, payload) in variants {
            let label = format!("{name}::{variant}");
            let payload = payload
                .iter()
                .map(|ty| enums.resolve_in(ty, &params).unwrap_or(Type::ONE))
                .collect::<Vec<_>>();
            enums.signatures.insert(label.clone(), (name.clone(), payload));
            enums
                .unqualified
                .entry(variant.clone())
                .and_modify(|existing| *existing = None)
                .or_insert(Some(label));
        }
    }
    enums
}
