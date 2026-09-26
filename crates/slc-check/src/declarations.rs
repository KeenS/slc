//! What the program declares: the variants of each `enum`, the payload each
//! variant carries, the fields of each `data` — and how a written type
//! resolves against them.

use slc_core::types::{Effect, Row, Type};
use slc_syntax::ast::{Decl, EffectRow, ParamPolarity, Program, TypeExpr};
use slc_syntax::lower::lower_type;
use std::collections::HashMap;

/// A written row as a type carries it: its effects, and its row variable
/// wherever `tail` can say which one the name stands for.
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
    /// Declaration name → each type parameter and the polarity it declares,
    /// in order; `None` where it declares none.
    param_signs: HashMap<String, Vec<(String, Option<ParamPolarity>)>>,
    /// Operation name → the effect that declares it.
    pub(crate) op_effects: HashMap<String, String>,
    /// Qualified `hand` names. A hand is installed with `do`, not used as a value.
    pub(crate) hands: std::collections::HashSet<String>,
    pub(crate) effects: std::collections::HashSet<String>,
    /// Menu or form name → the latent row it declares: what demanding an
    /// item, or feeding the form, performs.
    pub(crate) latent_rows: HashMap<String, Row>,
    /// Menu or form name → the position of the row parameter its own row
    /// names, `menu Seq<+T, E> / {..E}`: its latent row is that argument.
    latent_row_params: HashMap<String, usize>,
    /// `Walk::Item` → the trait, the associated type, and how many type
    /// arguments the trait takes. The projection is applied to those and
    /// then to the implementing type.
    projections: HashMap<String, (String, String, usize)>,
}

impl Declarations {
    pub(crate) fn resolve_row(
        &self,
        row: &EffectRow,
        tail: impl Fn(&str) -> Option<usize>,
        resolve: impl Fn(&TypeExpr) -> Option<Type>,
    ) -> Option<Row> {
        if row.tails.len() > 1 {
            return None;
        }
        let mut effects = std::collections::BTreeSet::new();
        for effect in &row.effects {
            let (name, args) = match &effect.kind {
                TypeExpr::Base(name) => (name, &[][..]),
                TypeExpr::Apply(name, args) => (name, args.as_slice()),
                _ => return None,
            };
            if (!self.effects.contains(name) && name != "IO")
                || args.len() != self.arity(name)
                || !self.args_match_kinds(name, args)
                || effects.iter().any(|effect: &Effect| effect.name == *name)
            {
                return None;
            }
            let args =
                args.iter().map(|argument| resolve(&argument.kind)).collect::<Option<Vec<_>>>()?;
            for (argument, (_, sign)) in args.iter().zip(self.param_signs(name)) {
                if matches!(sign, Some(ParamPolarity::Positive))
                    && argument.is_negative()
                    && !argument.is_positive()
                    || matches!(sign, Some(ParamPolarity::Negative))
                        && argument.is_positive()
                        && !argument.is_negative()
                {
                    return None;
                }
            }
            effects.insert(Effect { name: name.clone(), args });
        }
        Some(Row { effects, tail: row.tails.first().and_then(|name| tail(name)) })
    }

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
            TypeExpr::Base(name) if self.projections.contains_key(name) => return None,
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
                if self.projection_arity(name).is_some() {
                    let args =
                        args.iter().map(|arg| resolve(&arg.kind)).collect::<Option<Vec<_>>>()?;
                    return self.projected_type(name, args);
                }
                if self.effects.contains(name) || !self.args_match_kinds(name, args) {
                    return None;
                }
                let args = args.iter().map(|a| resolve(&a.kind)).collect::<Option<Vec<_>>>()?;
                let args = self.complete_args(name, args)?;
                if name == "Delayed" {
                    let inner = args.first()?;
                    if inner.is_positive() && !inner.is_negative() {
                        return None;
                    }
                    let row = match args.get(1)? {
                        Type::Rowed(_, row) => row.clone(),
                        _ => Row::default(),
                    };
                    Type::delayed(inner.clone(), row)
                } else if self.is_negative_decl(name) {
                    Type::Dual(Box::new(Type::Named(name.clone(), args)))
                } else {
                    Type::Named(name.clone(), args)
                }
            }
            // A row argument, carried on the unit. Its row variable is one of
            // the declaration's parameters, by position, as on a written
            // effect row, and `instantiate` puts each use's row in its place;
            // a row variable nothing here names does not resolve.
            TypeExpr::Row(row) => {
                if row.tails.iter().any(|tail| !params.contains_key(tail)) {
                    return None;
                }
                Type::rowed(
                    Type::ONE,
                    self.resolve_row(row, |tail| params.get(tail).copied(), resolve)?,
                )
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
            // `A -> B` is `(dual(A) ; B)`.
            TypeExpr::Fun(a, b) => Type::arrow(resolve(&a.kind)?, resolve(&b.kind)?),
            // `dual(A)` applies the involution; only a declaration's name
            // stays wrapped, because it is opaque to the core.
            TypeExpr::Dual(inner) => resolve(&inner.kind)?.dual(),
            // The effect row rides on the type it is written on; a row
            // variable is one of the declaration's parameters, by position.
            TypeExpr::Effectful(inner, row) => Type::rowed(
                resolve(&inner.kind)?,
                self.resolve_row(row, |tail| params.get(tail).copied(), resolve)?,
            ),
            other => return lower_type(other).ok(),
        };
        Some(resolved)
    }

    /// Record `Trait::Item` as a projection. `params` is the trait's own
    /// type-parameter count; the projection takes those and then `Self`.
    pub(crate) fn note_projection(&mut self, trait_name: &str, item: &str, params: usize) {
        self.projections.insert(
            format!("{trait_name}::{item}"),
            (trait_name.to_string(), item.to_string(), params),
        );
    }

    /// How many arguments `Trait::Item` takes, when it is a projection:
    /// the trait's parameters, plus the implementing type.
    pub(crate) fn projection_arity(&self, name: &str) -> Option<usize> {
        self.projections.get(name).map(|(_, _, params)| params + 1)
    }

    /// The nominal projection, when `args` is the trait's parameters plus
    /// the implementing type.
    pub(crate) fn projected_type(&self, name: &str, args: Vec<Type>) -> Option<Type> {
        let (trait_name, item, params) = self.projections.get(name)?;
        if args.len() != params + 1 {
            return None;
        }
        Some(Type::Named(slc_syntax::traits::assoc_type_name(trait_name, item), args))
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

    /// A declaration's type parameters and the polarity each declares.
    pub(crate) fn param_signs(&self, name: &str) -> &[(String, Option<ParamPolarity>)] {
        self.param_signs.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether a declaration's parameter at `index` is a row: one declared
    /// without a sign, as a function's row parameters are.
    pub(crate) fn is_row_param(&self, name: &str, index: usize) -> bool {
        if name == "Handler" {
            return index >= 2;
        }
        self.param_signs(name).get(index).is_some_and(|(_, sign)| sign.is_none())
    }

    /// Whether each written argument is of its parameter's kind: a row for a
    /// row parameter, a type for a type parameter. A type in a row's slot
    /// would stand for the row as an unknown nothing constrains.
    pub(crate) fn args_match_kinds(
        &self,
        name: &str,
        args: &[slc_syntax::ast::Node<TypeExpr>],
    ) -> bool {
        args.iter().enumerate().all(|(index, arg)| {
            self.is_row_param(name, index) == matches!(arg.kind, TypeExpr::Row(_))
        })
    }

    /// The first argument of the wrong kind in a written type, at any depth,
    /// described: what the declaration declares there, and what to write.
    pub(crate) fn row_kind_mismatch(&self, ty: &TypeExpr) -> Option<String> {
        let inner = |items: &[slc_syntax::ast::Node<TypeExpr>]| {
            items.iter().find_map(|item| self.row_kind_mismatch(&item.kind))
        };
        match ty {
            TypeExpr::Apply(name, args) => {
                for (index, arg) in args.iter().enumerate() {
                    let is_row = matches!(arg.kind, TypeExpr::Row(_));
                    let (param, sign) = self.param_signs(name).get(index)?;
                    if self.is_row_param(name, index) == is_row {
                        continue;
                    }
                    return Some(if is_row {
                        let mark = sign.map(|s| s.mark().to_string()).unwrap_or_default();
                        format!(
                            "gives `{name}` a row as argument {}, and `{name}` declares \
                             `<{mark}{param}>` there: write a type",
                            index + 1
                        )
                    } else {
                        format!(
                            "gives `{name}` a type as argument {}, and `{name}` declares the row \
                             parameter `{param}` there: write a row, `..E` or `{{IO}}`",
                            index + 1
                        )
                    });
                }
                inner(args)
            }
            TypeExpr::Tensor(items)
            | TypeExpr::Par(items)
            | TypeExpr::With(items)
            | TypeExpr::Sum(items) => inner(items),
            TypeExpr::Positive(i)
            | TypeExpr::Negative(i)
            | TypeExpr::Dual(i)
            | TypeExpr::Effectful(i, _) => self.row_kind_mismatch(&i.kind),
            TypeExpr::Fun(a, b) => {
                self.row_kind_mismatch(&a.kind).or_else(|| self.row_kind_mismatch(&b.kind))
            }
            TypeExpr::Base(_) | TypeExpr::Row(_) => None,
        }
    }

    /// A declaration's arguments as written, with row arguments left out at
    /// the end taken as the empty row: `Seq<i64>` is `Seq<i64, {}>`. Any other
    /// count than the declaration's is no type.
    pub(crate) fn complete_args(&self, name: &str, mut args: Vec<Type>) -> Option<Vec<Type>> {
        let arity = *self.arities.get(name)?;
        if args.len() > arity {
            return None;
        }
        while args.len() < arity {
            if !self.is_row_param(name, args.len()) {
                return None;
            }
            args.push(Type::ONE);
        }
        Some(args)
    }

    /// The position of the row parameter a menu's or form's own row names.
    pub(crate) fn latent_row_param(&self, name: &str) -> Option<usize> {
        self.latent_row_params.get(name).copied()
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
        let mut answer = self.destructor(label)?.1.first()?;
        while let Type::Delayed(inner, _) | Type::Rowed(inner, _) | Type::Dual(inner) = answer {
            answer = inner;
        }
        match answer {
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
    enums.declarations.insert("Delayed".into());
    enums.arities.insert("Delayed".into(), 2);
    enums.param_signs.insert(
        "Delayed".into(),
        vec![("T".into(), Some(ParamPolarity::Negative)), ("E".into(), None)],
    );
    enums.declarations.insert("Handler".into());
    enums.arities.insert("Handler".into(), 4);
    enums.param_signs.insert(
        "Handler".into(),
        ["A", "B", "E", "F"].into_iter().map(|name| (name.into(), None)).collect(),
    );
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
        if let Decl::Effect { name, operations, .. } = &d.kind {
            enums.effects.insert(name.clone());
            for op in operations {
                enums.op_effects.insert(op.name.clone(), name.clone());
            }
        }
        if let Decl::Hand { name, .. } = &d.kind {
            enums.hands.insert(name.clone());
        }
        // A row variable in a menu's or form's own row is one of its row
        // parameters, instantiated at each use.
        if let Decl::Menu { name, effects, type_params, type_param_signs, .. }
        | Decl::Form { name, effects, type_params, type_param_signs, .. } = &d.kind
            && let Some(tail) = effects.tails.first()
            && let Some(index) = type_params.iter().position(|param| param == tail)
            && type_param_signs.iter().all(|(signed, _)| signed != tail)
        {
            enums.latent_row_params.insert(name.clone(), index);
        }
        if let Decl::Form { name, .. } = &d.kind {
            enums.forms.insert(name.clone());
        }
        if let Decl::Data { name, type_params, type_param_signs, .. }
        | Decl::Enum { name, type_params, type_param_signs, .. }
        | Decl::Menu { name, type_params, type_param_signs, .. }
        | Decl::Form { name, type_params, type_param_signs, .. }
        | Decl::Effect { name, type_params, type_param_signs, .. } = &d.kind
        {
            enums.arities.insert(name.clone(), type_params.len());
            let signs = type_params
                .iter()
                .map(|param| {
                    let sign = type_param_signs.iter().find(|(n, _)| n == param).map(|(_, s)| *s);
                    (param.clone(), sign)
                })
                .collect();
            enums.param_signs.insert(name.clone(), signs);
        }
    }
    for declaration in &p.decls {
        if let Decl::Menu { name, effects, type_params, .. }
        | Decl::Form { name, effects, type_params, .. } = &declaration.kind
        {
            let params =
                type_params.iter().enumerate().map(|(index, name)| (name.clone(), index)).collect();
            if let Some(row) =
                enums.resolve_row(effects, |_| None, |ty| enums.resolve_in(ty, &params))
                && !row.is_empty()
            {
                enums.latent_rows.insert(name.clone(), row);
            }
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
