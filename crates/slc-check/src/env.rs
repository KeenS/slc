//! The checking environment: local bindings over the program's constants and
//! function signatures, together with the unification state.

use crate::signatures::FunctionSignature;
use slc_core::types::Type;
use slc_core::typing::Unification;
use slc_syntax::ast::{Decl, Program};
use slc_syntax::lower::lower_type;
use slc_syntax::traits::TraitInfo;
use std::collections::HashMap;

/// A bound the enclosing declaration established: `T: Into<String>` is the
/// rigid variable of `T`, the trait, the arguments, and the key those
/// arguments render as in the dictionary parameter's name.
#[derive(Debug, Clone)]
pub(crate) struct BoundInScope {
    pub(crate) var: usize,
    pub(crate) trait_name: String,
    pub(crate) type_param: String,
    pub(crate) args: Vec<Type>,
    pub(crate) arg_key: String,
    /// Associated types this bound pins, `Item = i64`.
    pub(crate) pins: Vec<(String, Type)>,
}

/// A local binding: its type, and — for a `let` of a value form — the
/// variables it generalizes. Every use instantiates those afresh; a
/// monomorphic binding generalizes nothing.
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) ty: Type,
    pub(crate) generalized: std::rc::Rc<Vec<usize>>,
}

/// One bound of a call awaiting its dictionary: the trait, the type that
/// must satisfy it, and the types the trait is applied to.
#[derive(Debug, Clone)]
pub(crate) struct PendingBound {
    pub(crate) trait_name: String,
    pub(crate) var: Type,
    pub(crate) args: Vec<Type>,
    /// Pins, freshened with the call's template variables.
    pub(crate) pins: Vec<(String, Type)>,
}

/// One bounded call awaiting its dictionaries: where it stands, who it
/// calls, and each bound with the type standing for its parameter.
#[derive(Debug, Clone)]
pub(crate) struct PendingDicts {
    pub(crate) span: slc_syntax::token::Span,
    pub(crate) callee: String,
    pub(crate) bounds: Vec<PendingBound>,
}

/// One use of a declaration's type parameter awaiting the type it is given:
/// `<+T>` takes only positive types and `<-T>` only negative ones, and which
/// `T` got is known once the declaration's unification has finished.
#[derive(Debug, Clone)]
pub(crate) struct PendingSign {
    pub(crate) span: slc_syntax::token::Span,
    /// The function, command or type declaration whose parameter it is.
    pub(crate) owner: String,
    pub(crate) param: String,
    pub(crate) sign: slc_syntax::ast::ParamPolarity,
    pub(crate) ty: Type,
}

/// One `::i(v)` awaiting its sum: the position is read against the type the
/// context gives it, which only the finished declaration knows.
#[derive(Debug, Clone)]
pub(crate) struct PendingInjection {
    pub(crate) span: slc_syntax::token::Span,
    pub(crate) index: usize,
    pub(crate) payload: Type,
    pub(crate) sum: Type,
}

/// One form value awaiting its components' polarities, which decide which
/// way each of its cuts faces.
#[derive(Debug, Clone)]
pub(crate) struct PendingPar {
    pub(crate) span: slc_syntax::token::Span,
    pub(crate) components: Vec<Type>,
}

/// One trait-method call awaiting its `Self` or its trait arguments. A
/// negative method's `Self` appears only in what it consumes, and a trait
/// parameter may appear only in what the call produces, so both are fixed
/// after the call, by the cut or by the type the call is expected to have.
#[derive(Debug, Clone)]
pub(crate) struct PendingMethod {
    pub(crate) span: slc_syntax::token::Span,
    pub(crate) method: String,
    pub(crate) trait_name: String,
    pub(crate) self_ty: Type,
    /// One fresh variable per trait parameter, in declaration order.
    pub(crate) trait_args: Vec<Type>,
    pub(crate) trait_param_names: Vec<String>,
}

/// Why a row constraint with a concrete or rigid bound was recorded, for the
/// diagnostic when it does not hold. A constraint without one was recorded
/// where a value met its slot.
#[derive(Debug, Clone)]
pub(crate) enum RowOrigin {
    Returned {
        name: String,
        ty: Type,
        span: slc_syntax::token::Span,
    },
    /// A declaration's body against the row it writes.
    Declaration {
        name: String,
        span: slc_syntax::token::Span,
    },
    /// A menu or form arm against the latent row its declaration writes.
    Latent {
        decl: String,
        span: slc_syntax::token::Span,
    },
    /// An argument against the parameter it is passed to: what the argument
    /// performs when run against the row the parameter's type allows.
    Argument {
        callee: String,
        param: String,
        argument: Option<String>,
        span: slc_syntax::token::Span,
    },
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
    /// Bounds in scope, from the enclosing declaration's `<T: Trait<…>>`.
    /// The parameter's name and the rendered arguments identify the
    /// dictionary a bounded call forwards or a method call projects from.
    pub(crate) bounds: Vec<BoundInScope>,
    /// The enclosing declaration's type parameters, each mapped to the rigid
    /// variable standing for it. A written type in *body* position — a
    /// lambda's annotation, a `let`'s, a scrutinee's — resolves through this
    /// first, so `T` inside the body is the same `T` the signature bound and
    /// not a fresh name that happens to look alike.
    pub(crate) rigid_vars: HashMap<String, Type>,
    /// Bounded calls whose dictionaries are not solved yet. A call standing
    /// in consumer position learns its type parameter from the cut it is
    /// part of — `42 | emit(s)` fixes `emit`'s `T` only when the cut is
    /// checked, after the call — so solving waits until the declaration's
    /// unification has finished.
    pub(crate) pending_dicts: Vec<PendingDicts>,
    /// Uses of signed type parameters whose types are not solved yet, for
    /// the same reason as `pending_dicts`.
    pub(crate) pending_signs: Vec<PendingSign>,
    /// Plain `let`s of computations, by the span of the computation and the
    /// type it binds: a negative one is delayed, which only the finished
    /// declaration's unification can tell.
    pub(crate) pending_lets: Vec<(slc_syntax::token::Span, Type)>,
    /// Computations in by-name positions — a tuple component, a bundle
    /// item, an argument — by span: a negative one is delayed once the
    /// declaration's unification says it is negative.
    pub(crate) pending_by_name: Vec<slc_syntax::token::Span>,
    /// Every name used, by span: one of type `(;)` that stands as a command
    /// is run there, which lowering does where the position is a command's.
    pub(crate) pending_names: Vec<slc_syntax::token::Span>,
    /// Unannotated lambda parameters — the lambda's span, the name and the
    /// type inference gave it — whose polarity must be known by the end of
    /// the declaration.
    pub(crate) pending_params: Vec<(slc_syntax::token::Span, String, Type)>,
    /// The type each checked expression was found to have, by span, for
    /// what is settled at the end of the declaration.
    pub(crate) expr_types: HashMap<slc_syntax::token::Span, Type>,
    /// The polarity each rigid variable's type parameter declares, so a use
    /// inside a generic body passes `T` on only where its mark allows.
    pub(crate) rigid_signs: HashMap<usize, slc_syntax::ast::ParamPolarity>,
    /// Trait-method calls whose dispatch is not resolved yet, for the same
    /// reason as `pending_dicts`.
    pub(crate) pending_methods: Vec<PendingMethod>,
    /// Injections whose sum is not known yet, for the same reason.
    pub(crate) pending_injections: Vec<PendingInjection>,
    /// Form values whose components' types are not settled yet.
    pub(crate) pending_pars: Vec<PendingPar>,
    pub(crate) pending_consumers: Vec<(slc_syntax::token::Span, Type)>,
    /// `mu` scrutinees whose type was still a variable while checking.
    /// Settled with the rest of the declaration: a `mu` consumes positive data.
    pub(crate) pending_scrutinees: Vec<(slc_syntax::token::Span, Type)>,
    /// What lowering needs to dispatch traits without a runtime method value:
    /// how each trait-method call resolves, and the dictionaries each call to
    /// a bounded function must pass.
    pub(crate) dispatch: slc_syntax::lower::DispatchInfo,
    pub(crate) declarations: Option<&'a crate::declarations::Declarations>,
    /// The row variable of the body being checked: what running it performs.
    /// `None` outside every body.
    pub(crate) current_row: Option<usize>,
    /// Row constraint position → why it was recorded.
    pub(crate) row_origins: HashMap<usize, RowOrigin>,
    /// Rigid row variable → the name its declaration writes, `E` for `..E`.
    pub(crate) row_names: HashMap<usize, String>,
    /// What the solved rows refuse, one diagnostic each.
    pub(crate) row_diagnostics: Vec<crate::Diagnostic>,
    pub(crate) elaboration_origins: HashMap<slc_syntax::token::Span, slc_syntax::token::Span>,
    pub(crate) polarity_hints: HashMap<slc_syntax::token::Span, slc_syntax::ast::ParamPolarity>,
    pub(crate) pending_computations:
        Vec<(slc_syntax::token::Span, Type, Option<slc_syntax::ast::ParamPolarity>)>,
    /// Generic uses whose type arguments are solved when the declaration is.
    pub(crate) pending_insts: Vec<PendingInst>,
    /// The function whose body is being checked. A use inside it is solved again per copy.
    pub(crate) enclosing: Option<String>,
    /// `(function, name, type)` as bound. The type may still mention that function's parameters.
    pub(crate) bound_words: Vec<(String, String, Type)>,
    /// Function → the rigid variable of each type parameter, so a copy can substitute its bindings.
    pub(crate) fn_context: HashMap<String, Vec<(String, usize)>>,
    /// Nonzero inside a lambda expression. Its binders are not slots of the enclosing frame,
    /// and recording them would hide the outer word that does occupy the slot.
    pub(crate) word_depth: u32,
}

/// A use of a generic declaration, recorded before unification finishes.
#[derive(Debug, Clone)]
pub(crate) struct PendingInst {
    pub(crate) span: slc_syntax::token::Span,
    pub(crate) decl: String,
    pub(crate) param_names: Vec<String>,
    pub(crate) param_types: Vec<Type>,
    /// Type-parameter name and the fresh variable standing for it.
    pub(crate) args: Vec<(String, Type)>,
    pub(crate) result: Option<Type>,
    /// The generic function whose body contains the use. Absent at the top level.
    pub(crate) owner: Option<String>,
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
            rigid_vars: HashMap::new(),
            pending_dicts: Vec::new(),
            pending_signs: Vec::new(),
            pending_lets: Vec::new(),
            pending_by_name: Vec::new(),
            pending_names: Vec::new(),
            pending_params: Vec::new(),
            expr_types: HashMap::new(),
            rigid_signs: HashMap::new(),
            pending_methods: Vec::new(),
            pending_injections: Vec::new(),
            pending_pars: Vec::new(),
            pending_consumers: Vec::new(),
            pending_scrutinees: Vec::new(),
            dispatch: slc_syntax::lower::DispatchInfo::default(),
            declarations: None,
            current_row: None,
            row_origins: HashMap::new(),
            row_names: HashMap::new(),
            row_diagnostics: Vec::new(),
            elaboration_origins: HashMap::new(),
            polarity_hints: HashMap::new(),
            pending_computations: Vec::new(),
            pending_insts: Vec::new(),
            enclosing: None,
            bound_words: Vec::new(),
            fn_context: HashMap::new(),
            word_depth: 0,
        }
    }

    /// What running `row` performs happens here, in the body being checked.
    pub(crate) fn perform(&mut self, row: slc_core::types::Row) {
        if let Some(current) = self.current_row {
            let here = slc_core::types::Row { effects: Default::default(), tail: Some(current) };
            self.uni.constrain_row(row, here);
        }
    }

    /// Give every row constraint recorded since `from` the origin `origin`,
    /// unless it already has one.
    pub(crate) fn tag_rows_since(&mut self, from: usize, origin: RowOrigin) {
        for index in from..self.uni.row_constraints().len() {
            self.row_origins.entry(index).or_insert_with(|| origin.clone());
        }
    }

    /// Record that `sub` fits inside `sup`, and why, for its diagnostic.
    pub(crate) fn constrain_row_for(
        &mut self,
        sub: slc_core::types::Row,
        sup: slc_core::types::Row,
        origin: RowOrigin,
    ) {
        let index = self.uni.row_constraints().len();
        self.uni.constrain_row(sub, sup);
        if self.uni.row_constraints().len() > index {
            self.row_origins.insert(index, origin);
        }
    }

    pub(crate) fn push(&mut self) {
        self.locals.push(HashMap::new());
    }

    pub(crate) fn pop(&mut self) {
        self.locals.pop();
    }

    pub(crate) fn define(&mut self, name: &str, ty: Type) {
        self.note_word(name, &ty);
        if let Some(frame) = self.locals.last_mut() {
            frame.insert(
                name.to_string(),
                Binding { ty, generalized: std::rc::Rc::new(Vec::new()) },
            );
        }
    }

    /// The first binding of `name` in the enclosing function. A later shadow
    /// does not replace it: the native slot is the first one too.
    fn note_word(&mut self, name: &str, ty: &Type) {
        if self.word_depth != 0 {
            return;
        }
        let Some(owner) = self.enclosing.clone() else { return };
        if self.bound_words.iter().any(|(fn_name, word, _)| fn_name == &owner && word == name) {
            return;
        }
        self.bound_words.push((owner, name.to_string(), ty.clone()));
    }

    /// A `let` of a value form: the listed variables are the binding's own,
    /// and every use gets fresh copies of them.
    pub(crate) fn define_scheme(&mut self, name: &str, ty: Type, generalized: Vec<usize>) {
        self.note_word(name, &ty);
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
pub(crate) fn replace_vars(ty: &Type, map: &HashMap<usize, Type>) -> Type {
    match ty {
        Type::Var(v) => map.get(v).cloned().unwrap_or_else(|| ty.clone()),
        Type::Tensor(items) => Type::Tensor(items.iter().map(|x| replace_vars(x, map)).collect()),
        Type::Par(items) => Type::Par(items.iter().map(|x| replace_vars(x, map)).collect()),
        Type::With(items) => Type::With(items.iter().map(|x| replace_vars(x, map)).collect()),
        Type::Sum(items) => Type::Sum(items.iter().map(|x| replace_vars(x, map)).collect()),
        Type::Dual(t) => Type::Dual(Box::new(replace_vars(t, map))),
        Type::Named(name, args) => {
            Type::Named(name.clone(), args.iter().map(|a| replace_vars(a, map)).collect())
        }
        Type::Rowed(t, row) => {
            Type::rowed(replace_vars(t, map), row.map_types(|arg| replace_vars(arg, map)))
        }
        Type::Delayed(inner, row) => {
            Type::delayed(replace_vars(inner, map), row.map_types(|arg| replace_vars(arg, map)))
        }
        atom => atom.clone(),
    }
}

pub(crate) fn collect_vars(ty: &Type, out: &mut std::collections::HashSet<usize>) {
    match ty {
        Type::Var(v) => {
            out.insert(*v);
        }
        Type::Tensor(items) | Type::Par(items) | Type::With(items) | Type::Sum(items) => {
            for item in items {
                collect_vars(item, out);
            }
        }
        Type::Dual(t) => {
            collect_vars(t, out);
        }
        Type::Rowed(inner, row) | Type::Delayed(inner, row) => {
            collect_vars(inner, out);
            for argument in row.effects.iter().flat_map(|effect| &effect.args) {
                collect_vars(argument, out);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use slc_core::types::{Effect, Row};

    #[test]
    fn schemes_collect_and_replace_variables_inside_effect_arguments() {
        let nested = Type::rowed(
            Type::Var(2),
            Row {
                effects: [Effect { name: "Nested".into(), args: vec![Type::Var(3)] }].into(),
                tail: Some(9),
            },
        );
        let ty = Type::delayed(
            Type::Var(1),
            Row {
                effects: [Effect { name: "Build".into(), args: vec![nested] }].into(),
                tail: None,
            },
        );
        let mut variables = Default::default();
        collect_vars(&ty, &mut variables);
        assert_eq!(variables, [1, 2, 3].into());
        let replaced = replace_vars(&ty, &[(3, Type::ONE)].into());
        variables.clear();
        collect_vars(&replaced, &mut variables);
        assert_eq!(variables, [1, 2].into());
        assert!(replaced.to_string().contains("Nested<(,)>"));
        assert!(replaced.to_string().contains("..?9"));
    }
}
