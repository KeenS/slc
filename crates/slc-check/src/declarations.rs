//! What the program declares: the variants of each `enum`, the payload each
//! variant carries, the fields of each `struct` — and how a written type
//! resolves against them.

use slc_core::types::Type;
use slc_syntax::ast::{Decl, Program, TypeExpr};
use slc_syntax::lower::lower_type;
use std::collections::HashMap;

/// What the checker knows about the program's type declarations: the variants
/// of each `enum`, the payload each variant carries, and the fields of each
/// `struct`.
#[derive(Debug, Default)]
pub struct Declarations {
    /// Every declaration name in the program: `struct` and `enum` alike.
    pub(crate) declarations: std::collections::HashSet<String>,
    /// Declaration name → variant names, in declaration order.
    variants: HashMap<String, Vec<String>>,
    /// Fully qualified label → declaration name and payload types.
    signatures: HashMap<String, (String, Vec<Type>)>,
    /// Unqualified variant name → its label, when only one enum declares it.
    unqualified: HashMap<String, Option<String>>,
    /// Declaration name → fields, in declaration order.
    pub(crate) structs: HashMap<String, Vec<(String, Type)>>,
    /// Declared `menu` names. A menu's items live in `variants` and
    /// `signatures` like an enum's variants — the request view — with each
    /// item's payload being the consumer of its answer.
    pub(crate) menus: std::collections::HashSet<String>,
}

impl Declarations {
    /// Lower a written type, resolving a declaration name to its named type.
    /// `lower_type` only knows the built-in types, so `struct` and `enum`
    /// names have to be resolved here — including under a sign or a
    /// connective, so `-ParseResult` is a consumer of a declared type.
    pub(crate) fn resolve(&self, ty: &TypeExpr) -> Option<Type> {
        let resolved = match ty {
            // A menu name denotes the negative additive itself; its dual —
            // the bare `Named` — is the positive type of its requests.
            TypeExpr::Base(name) if self.menus.contains(name) => {
                Type::Dual(Box::new(Type::Named(name.clone())))
            }
            TypeExpr::Base(name) if self.declarations.contains(name) => Type::Named(name.clone()),
            TypeExpr::Positive(inner) => self.resolve(&inner.kind)?,
            TypeExpr::Negative(inner) if !matches!(inner.kind, TypeExpr::Bottom) => {
                self.resolve(&inner.kind)?.dual()
            }
            TypeExpr::Tensor(a, b) => {
                Type::Tensor(Box::new(self.resolve(&a.kind)?), Box::new(self.resolve(&b.kind)?))
            }
            TypeExpr::Par(a, b) => {
                Type::Par(Box::new(self.resolve(&a.kind)?), Box::new(self.resolve(&b.kind)?))
            }
            // `A → B` is `-A ⅋ B`.
            TypeExpr::Fun(a, b) => Type::arrow(self.resolve(&a.kind)?, self.resolve(&b.kind)?),
            TypeExpr::List(inner) => Type::List(Box::new(self.resolve(&inner.kind)?)),
            // `dual(A)` applies the involution; only a declaration's name
            // stays wrapped, because it is opaque to the core.
            TypeExpr::Dual(inner) => self.resolve(&inner.kind)?.dual(),
            TypeExpr::Down(inner) => Type::Down(Box::new(self.resolve(&inner.kind)?)),
            TypeExpr::Up(inner) => Type::Up(Box::new(self.resolve(&inner.kind)?)),
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
        Some(self.structs.get(name)?.iter().map(|(_, ty)| ty.clone()).collect())
    }

    /// Resolve a variant path or an unambiguous unqualified variant name.
    /// Whether a name is a declared `struct` or `enum`.
    pub(crate) fn declares(&self, name: &str) -> bool {
        self.declarations.contains(name)
    }

    /// Whether a name is a declared `menu`.
    pub(crate) fn is_menu(&self, name: &str) -> bool {
        self.menus.contains(name)
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
        if let Decl::Struct { name, .. } | Decl::Enum { name, .. } | Decl::Menu { name, .. } =
            &d.kind
        {
            enums.declarations.insert(name.clone());
        }
        if let Decl::Menu { name, .. } = &d.kind {
            enums.menus.insert(name.clone());
        }
        if let Decl::Struct { name, fields } = &d.kind {
            enums.structs.insert(
                name.clone(),
                fields
                    .iter()
                    .map(|(field, ty)| (field.clone(), lower_type(ty).unwrap_or(Type::One)))
                    .collect(),
            );
        }
    }
    for d in &p.decls {
        // A menu registers the request view of itself: one "variant" per
        // item, labelled `Menu::item`, whose payload is the consumer of the
        // item's answer — the continuation a request carries.
        if let Decl::Menu { name, items } = &d.kind {
            enums.variants.insert(name.clone(), items.iter().map(|(i, _)| i.clone()).collect());
            for (item, answer) in items {
                let label = format!("{name}::{item}");
                let answer = enums.resolve(answer).unwrap_or(Type::One);
                enums.signatures.insert(label.clone(), (name.clone(), vec![answer.dual()]));
                enums
                    .unqualified
                    .entry(item.clone())
                    .and_modify(|existing| *existing = None)
                    .or_insert(Some(label));
            }
            continue;
        }
        let Decl::Enum { name, variants } = &d.kind else { continue };
        enums.variants.insert(name.clone(), variants.iter().map(|(v, _)| v.clone()).collect());
        for (variant, payload) in variants {
            let label = format!("{name}::{variant}");
            let payload =
                payload.iter().map(|ty| enums.resolve(ty).unwrap_or(Type::One)).collect::<Vec<_>>();
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
