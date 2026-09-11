//! The checking environment: local bindings over the program's constants and
//! function signatures, together with the unification state.

use crate::signatures::FunctionSignature;
use slc_core::types::Type;
use slc_core::typing::Unification;
use slc_syntax::ast::{Decl, Program};
use slc_syntax::lower::lower_type;
use slc_syntax::traits::TraitInfo;
use std::collections::HashMap;

/// A local binding: its type, and — for a `let` of a value form — the
/// variables it generalizes. Every use instantiates those afresh; a
/// monomorphic binding generalizes nothing.
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) ty: Type,
    pub(crate) generalized: std::rc::Rc<Vec<usize>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Env<'a> {
    pub(crate) constants: &'a HashMap<String, Type>,
    pub(crate) functions: &'a HashMap<String, FunctionSignature>,
    pub(crate) locals: Vec<HashMap<String, Binding>>,
    /// The program-wide unification state. Everything the checker cannot
    /// read off an annotation is a variable here, solved by use — never a
    /// wildcard that fits anything.
    pub(crate) uni: Unification,
    /// The type the enclosing negative `fn` consumes — what follows its
    /// `<-`. A `select` in its body is the consumer of exactly that, so a
    /// `select` there need not repeat it.
    pub(crate) consumed: Option<Type>,
    /// The program's traits and impls.
    pub(crate) traits: &'a TraitInfo,
    /// Bounds in scope: a rigid type-variable index, the trait it is known to
    /// satisfy, and the type parameter's name — from the enclosing
    /// declaration's `<T: Trait>`. The name identifies the dictionary
    /// parameter a bounded call forwards or a method call projects from.
    pub(crate) bounds: Vec<(usize, String, String)>,
    /// What lowering needs to dispatch traits without a runtime method value:
    /// how each trait-method call resolves, and the dictionaries each call to
    /// a bounded function must pass.
    pub(crate) dispatch: slc_syntax::lower::DispatchInfo,
}

impl<'a> Env<'a> {
    pub(crate) fn root(
        constants: &'a HashMap<String, Type>,
        functions: &'a HashMap<String, FunctionSignature>,
        traits: &'a TraitInfo,
    ) -> Self {
        Self {
            constants,
            functions,
            locals: Vec::new(),
            uni: Unification::new(),
            consumed: None,
            traits,
            bounds: Vec::new(),
            dispatch: slc_syntax::lower::DispatchInfo::default(),
        }
    }

    pub(crate) fn push(&mut self) {
        self.locals.push(HashMap::new());
    }

    pub(crate) fn pop(&mut self) {
        self.locals.pop();
    }

    pub(crate) fn define(&mut self, name: &str, ty: Type) {
        if let Some(frame) = self.locals.last_mut() {
            frame.insert(
                name.to_string(),
                Binding { ty, generalized: std::rc::Rc::new(Vec::new()) },
            );
        }
    }

    /// A `let` of a value form: the listed variables are the binding's own,
    /// and every use gets fresh copies of them.
    pub(crate) fn define_scheme(&mut self, name: &str, ty: Type, generalized: Vec<usize>) {
        if let Some(frame) = self.locals.last_mut() {
            frame.insert(
                name.to_string(),
                Binding { ty, generalized: std::rc::Rc::new(generalized) },
            );
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Type> {
        for frame in self.locals.iter().rev() {
            if let Some(binding) = frame.get(name) {
                return Some(binding.ty.clone());
            }
        }
        self.constants.get(name).cloned()
    }

    /// The type of a use of `name`: a generalized binding's variables are
    /// instantiated afresh, so two uses can choose two types.
    pub(crate) fn lookup_instantiated(&mut self, name: &str) -> Option<Type> {
        let binding = self.locals.iter().rev().find_map(|frame| frame.get(name)).cloned()?;
        if binding.generalized.is_empty() {
            return Some(binding.ty);
        }
        let mut map: HashMap<usize, Type> = HashMap::new();
        for var in binding.generalized.iter() {
            map.insert(*var, self.uni.fresh_var());
        }
        Some(replace_vars(&binding.ty, &map))
    }

    /// Every type variable the environment claims, with the current
    /// substitution applied: a `let` may not generalize these.
    pub(crate) fn free_vars(&self) -> std::collections::HashSet<usize> {
        let mut out = std::collections::HashSet::new();
        for frame in &self.locals {
            for binding in frame.values() {
                collect_vars(&self.uni.apply(&binding.ty), &mut out);
            }
        }
        out
    }
}

/// Substitute exactly the listed variables — a scheme's own — leaving every
/// other variable shared.
fn replace_vars(ty: &Type, map: &HashMap<usize, Type>) -> Type {
    match ty {
        Type::Var(v) => map.get(v).cloned().unwrap_or_else(|| ty.clone()),
        Type::Tensor(a, b) => {
            Type::Tensor(Box::new(replace_vars(a, map)), Box::new(replace_vars(b, map)))
        }
        Type::Par(a, b) => {
            Type::Par(Box::new(replace_vars(a, map)), Box::new(replace_vars(b, map)))
        }
        Type::With(a, b) => {
            Type::With(Box::new(replace_vars(a, map)), Box::new(replace_vars(b, map)))
        }
        Type::Sum(a, b) => {
            Type::Sum(Box::new(replace_vars(a, map)), Box::new(replace_vars(b, map)))
        }
        Type::Dual(t) => Type::Dual(Box::new(replace_vars(t, map))),
        Type::List(t) => Type::List(Box::new(replace_vars(t, map))),
        Type::Down(t) => Type::Down(Box::new(replace_vars(t, map))),
        Type::Up(t) => Type::Up(Box::new(replace_vars(t, map))),
        Type::Named(name, args) => {
            Type::Named(name.clone(), args.iter().map(|a| replace_vars(a, map)).collect())
        }
        atom => atom.clone(),
    }
}

pub(crate) fn collect_vars(ty: &Type, out: &mut std::collections::HashSet<usize>) {
    match ty {
        Type::Var(v) => {
            out.insert(*v);
        }
        Type::Tensor(a, b) | Type::Par(a, b) | Type::With(a, b) | Type::Sum(a, b) => {
            collect_vars(a, out);
            collect_vars(b, out);
        }
        Type::Dual(t) | Type::List(t) | Type::Down(t) | Type::Up(t) => {
            collect_vars(t, out);
        }
        Type::Named(_, args) => {
            for arg in args {
                collect_vars(arg, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn constant_types(p: &Program) -> HashMap<String, Type> {
    let mut out: HashMap<String, Type> = HashMap::new();
    out.extend(constant_declarations(p));
    out
}

fn constant_declarations(p: &Program) -> HashMap<String, Type> {
    p.decls
        .iter()
        .filter_map(|d| {
            let Decl::Const { name, ty, .. } = &d.kind else {
                return None;
            };
            lower_type(ty).ok().map(|ty| (name.clone(), ty))
        })
        .collect()
}
