//! The checking environment: local bindings over the program's constants and
//! function signatures, together with the unification state.

use crate::signatures::FunctionSignature;
use slc_core::types::Type;
use slc_core::typing::Unification;
use slc_syntax::ast::{Decl, Program};
use slc_syntax::lower::lower_type;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(crate) struct Env<'a> {
    pub(crate) constants: &'a HashMap<String, Type>,
    pub(crate) functions: &'a HashMap<String, FunctionSignature>,
    pub(crate) locals: Vec<HashMap<String, Type>>,
    /// The program-wide unification state. Everything the checker cannot
    /// read off an annotation is a variable here, solved by use — never a
    /// wildcard that fits anything.
    pub(crate) uni: Unification,
    /// The type the enclosing negative `fn` consumes — what follows its
    /// `<-`. A `select` in its body is the consumer of exactly that, so a
    /// `select` there need not repeat it.
    pub(crate) consumed: Option<Type>,
}

impl<'a> Env<'a> {
    pub(crate) fn root(
        constants: &'a HashMap<String, Type>,
        functions: &'a HashMap<String, FunctionSignature>,
    ) -> Self {
        Self { constants, functions, locals: Vec::new(), uni: Unification::new(), consumed: None }
    }

    pub(crate) fn push(&mut self) {
        self.locals.push(HashMap::new());
    }

    pub(crate) fn pop(&mut self) {
        self.locals.pop();
    }

    pub(crate) fn define(&mut self, name: &str, ty: Type) {
        if let Some(frame) = self.locals.last_mut() {
            frame.insert(name.to_string(), ty);
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Type> {
        for frame in self.locals.iter().rev() {
            if let Some(ty) = frame.get(name) {
                return Some(ty.clone());
            }
        }
        self.constants.get(name).cloned()
    }

    pub(crate) fn current_continuation_names(&self) -> Vec<(String, Type)> {
        self.locals
            .iter()
            .rev()
            .find_map(|frame| {
                let names: Vec<_> = frame
                    .iter()
                    .filter(|(_, ty)| matches!(ty, Type::Neg(_) | Type::Par(..) | Type::Bottom))
                    .map(|(name, ty)| (name.clone(), ty.clone()))
                    .collect();
                (!names.is_empty()).then_some(names)
            })
            .unwrap_or_default()
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
