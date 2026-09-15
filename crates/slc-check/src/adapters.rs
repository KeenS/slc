use crate::declarations::Declarations;
use crate::env::Env;
use slc_core::types::{Row, Type};
use slc_core::typing::Unification;
use slc_syntax::ast::ParamPolarity;
use slc_syntax::lower::{Adapter, Swap, alternative_label};
use std::collections::HashMap;

pub(crate) fn derive(env: &mut Env, expected: &Type, actual: &Type) -> Option<usize> {
    let mut builder = Builder {
        declarations: env.declarations?,
        unification: env.uni.clone(),
        adapters: env.dispatch.adapters.clone(),
        active: HashMap::new(),
        duals: HashMap::new(),
        dualized: HashMap::new(),
        signs: env.rigid_signs.clone(),
    };
    let adapter = builder.build(expected, actual, false)?;
    env.uni = builder.unification;
    env.dispatch.adapters = builder.adapters;
    Some(adapter)
}

struct Builder<'a> {
    declarations: &'a Declarations,
    unification: Unification,
    adapters: Vec<Adapter>,
    active: HashMap<(Type, Type, bool), usize>,
    duals: HashMap<(Type, Type), usize>,
    dualized: HashMap<usize, usize>,
    signs: HashMap<usize, ParamPolarity>,
}

impl Builder<'_> {
    fn push(&mut self, adapter: Adapter) -> usize {
        let index = self.adapters.len();
        self.adapters.push(adapter);
        index
    }

    fn exact(&mut self, expected: &Type, actual: &Type, nested: bool) -> bool {
        let mut probe = self.unification.clone();
        let result = if nested {
            probe.unify(
                &Type::Named("$adapter_invariant".into(), vec![expected.clone()]),
                &Type::Named("$adapter_invariant".into(), vec![actual.clone()]),
            )
        } else {
            probe.unify(expected, actual)
        };
        if result.is_err() {
            return false;
        }
        self.unification = probe;
        true
    }

    fn rows(&mut self, expected: &Row, actual: &Row, nested: bool) {
        self.unification.constrain_row(actual.clone(), expected.clone());
        if nested {
            self.unification.constrain_row(expected.clone(), actual.clone());
        }
    }

    fn build(&mut self, expected: &Type, actual: &Type, nested: bool) -> Option<usize> {
        let expected = self.unification.apply(expected);
        let actual = self.unification.apply(actual);
        if self.exact(&expected, &actual, nested) {
            return Some(self.push(Adapter::Identity));
        }
        if let Some(adapter) = self.duals.get(&(expected.clone(), actual.clone())) {
            return Some(*adapter);
        }
        let key = (expected.clone(), actual.clone(), nested);
        if let Some(index) = self.active.get(&key) {
            return Some(*index);
        }
        if let (Type::Named(wanted, _), Type::Named(given, _)) = (&expected, &actual)
            && self.active.keys().any(|(previous_expected, previous_actual, _)| {
                matches!((previous_expected, previous_actual), (Type::Named(previous_wanted, _), Type::Named(previous_given, _))
                    if wanted == previous_wanted && given == previous_given
                        && (type_size(&expected) > type_size(previous_expected)
                            || type_size(&actual) > type_size(previous_actual)))
            })
        {
            return None;
        }
        if self.active.len() >= 128 {
            return None;
        }
        let index = self.push(Adapter::Identity);
        self.active.insert(key.clone(), index);
        let adapter = self.structure(&expected, &actual, nested)?;
        self.adapters[index] = adapter;
        self.active.remove(&key);
        Some(index)
    }

    fn structure(&mut self, expected: &Type, actual: &Type, nested: bool) -> Option<Adapter> {
        match (expected, actual) {
            (Type::Delayed(wanted, allowed), Type::Delayed(given, performed)) => {
                self.rows(allowed, performed, nested);
                return self.structure(wanted, given, nested);
            }
            (Type::Delayed(wanted, allowed), given) => {
                self.rows(allowed, &Row::default(), nested);
                return self.structure(wanted, given, nested);
            }
            (wanted, Type::Delayed(given, performed)) => {
                self.rows(&Row::default(), performed, nested);
                return self.structure(wanted, given, nested);
            }
            (Type::Rowed(wanted, allowed), Type::Rowed(given, performed)) => {
                self.rows(allowed, performed, nested);
                return self.structure(wanted, given, nested);
            }
            (Type::Rowed(wanted, allowed), given) => {
                let performed = self.unification.latent_row(given);
                self.rows(allowed, &performed, nested);
                return self.structure(wanted, given, nested);
            }
            (wanted, Type::Rowed(given, performed)) => {
                let allowed = self.unification.latent_row(wanted);
                self.rows(&allowed, performed, nested);
                return self.structure(wanted, given, nested);
            }
            _ => {}
        }
        match (expected, actual) {
            (Type::Tensor(wanted), Type::Tensor(given))
            | (Type::With(wanted), Type::With(given))
                if wanted.len() == given.len() =>
            {
                Some(Adapter::Product {
                    items: self.components(wanted, given)?,
                    additive: matches!(expected, Type::With(_)),
                })
            }
            (Type::Sum(wanted), Type::Sum(given)) if wanted.len() == given.len() => {
                let components = self.components(wanted, given)?;
                Some(Adapter::Tagged {
                    owner: "(|)".into(),
                    branches: components
                        .into_iter()
                        .enumerate()
                        .map(|(index, adapter)| (alternative_label(index), vec![adapter]))
                        .collect(),
                })
            }
            (Type::Par(wanted), Type::Par(given)) if wanted.len() == 2 && given.len() == 2 => {
                let before = self.unification.clone();
                let length = self.adapters.len();
                let active = self.active.clone();
                let duals = self.duals.clone();
                let dualized = self.dualized.clone();
                if let Some(adapter) = self.function(wanted, given) {
                    return Some(adapter);
                }
                self.unification = before;
                self.adapters.truncate(length);
                self.active = active;
                self.duals = duals;
                self.dualized = dualized;
                let left = &given[0];
                let right = &given[1];
                let reversed = vec![right.clone(), left.clone()];
                let function = self.function(wanted, &reversed)?;
                let turn = self.push(Adapter::Swap(Swap {
                    left_positive: self.positive(left)?,
                    right_positive: self.positive(right)?,
                }));
                let function = self.push(function);
                Some(Adapter::Compose(turn, function))
            }
            (Type::Named(wanted, want_args), Type::Named(given, give_args))
                if wanted == given && want_args.len() == give_args.len() =>
            {
                self.arguments(wanted, want_args, give_args)?;
                if let Some(fields) = self.declarations.fields(wanted) {
                    return Some(Adapter::Tagged {
                        owner: wanted.clone(),
                        branches: vec![(
                            wanted.clone(),
                            self.instantiated_components(&fields, want_args, give_args)?,
                        )],
                    });
                }
                let variants = self.declarations.variants_of(wanted)?;
                let mut branches = Vec::new();
                for variant in variants {
                    let label = format!("{wanted}::{variant}");
                    let (_, fields) = self.declarations.variant(&label)?;
                    branches
                        .push((label, self.instantiated_components(fields, want_args, give_args)?));
                }
                Some(Adapter::Tagged { owner: wanted.clone(), branches })
            }
            (Type::Dual(wanted), Type::Dual(given)) => {
                if let (Type::Named(want_name, want_args), Type::Named(give_name, give_args)) =
                    (wanted.as_ref(), given.as_ref())
                    && want_name == give_name
                    && self.declarations.is_menu(want_name)
                {
                    self.arguments(want_name, want_args, give_args)?;
                    let mut answers = Vec::new();
                    for item in self.declarations.variants_of(want_name)? {
                        let label = format!("{want_name}::{item}");
                        let (_, fields) = self.declarations.variant(&label)?;
                        let answer = fields.first()?.dual();
                        let adapter = self.build(
                            &answer.instantiate(want_args),
                            &answer.instantiate(give_args),
                            true,
                        )?;
                        answers.push((label, adapter));
                    }
                    return Some(Adapter::Menu { owner: want_name.clone(), answers });
                }
                if expected.is_negative() && !expected.is_positive() {
                    return Some(Adapter::Consumer { input: self.build(given, wanted, true)? });
                }
                None
            }
            _ => None,
        }
    }

    fn function(&mut self, wanted: &[Type], given: &[Type]) -> Option<Adapter> {
        let input = self.build(&given[0].dual(), &wanted[0].dual(), true)?;
        let output = self.build(&wanted[1], &given[1], true)?;
        Some(Adapter::Function { input, output })
    }

    fn positive(&self, ty: &Type) -> Option<bool> {
        match self.unification.apply(ty) {
            Type::Dual(inner) => self.positive(&inner).map(|positive| !positive),
            Type::Delayed(inner, _) | Type::Rowed(inner, _) => self.positive(&inner),
            Type::Var(index) => match self.signs.get(&index) {
                Some(ParamPolarity::Positive) => Some(true),
                Some(ParamPolarity::Negative) => Some(false),
                _ => None,
            },
            ty if ty.is_positive() != ty.is_negative() => Some(ty.is_positive()),
            _ => None,
        }
    }

    fn arguments(&mut self, name: &str, wanted: &[Type], given: &[Type]) -> Option<()> {
        for (index, (wanted, given)) in wanted.iter().zip(given).enumerate() {
            if self.declarations.is_row_param(name, index) {
                if !self.exact(wanted, given, true) {
                    return None;
                }
            } else {
                self.build(wanted, given, true)?;
                let reverse = self.build(given, wanted, true)?;
                let length = self.adapters.len();
                let dualized = self.dualized.clone();
                if let Some(adapter) = self.dualize(reverse) {
                    self.duals.insert((wanted.dual(), given.dual()), adapter);
                } else {
                    self.adapters.truncate(length);
                    self.dualized = dualized;
                }
            }
        }
        Some(())
    }

    fn dualize(&mut self, index: usize) -> Option<usize> {
        if let Some(adapter) = self.dualized.get(&index) {
            return Some(*adapter);
        }
        if matches!(self.adapters[index], Adapter::Identity) {
            return Some(index);
        }
        let output = self.push(Adapter::Identity);
        self.dualized.insert(index, output);
        let adapter = match self.adapters[index].clone() {
            Adapter::Identity => return Some(index),
            Adapter::Swap(swap) => Adapter::ReverseProduct(swap),
            Adapter::ReverseProduct(swap) => Adapter::Swap(swap),
            Adapter::Compose(first, second) => {
                Adapter::Compose(self.dualize(second)?, self.dualize(first)?)
            }
            Adapter::Function { input, output } => {
                Adapter::Product { items: vec![input, self.dualize(output)?], additive: false }
            }
            Adapter::Consumer { input } => Adapter::Compose(input, self.push(Adapter::Identity)),
            Adapter::Product { items, additive: false } if items.len() == 2 => {
                Adapter::Function { input: items[0], output: self.dualize(items[1])? }
            }
            Adapter::Product { items, additive: true } => {
                let mut branches = Vec::new();
                for (position, item) in items.into_iter().enumerate() {
                    branches.push((alternative_label(position), vec![self.dualize(item)?]));
                }
                Adapter::Tagged { owner: "(|)".into(), branches }
            }
            Adapter::Tagged { owner, branches } if owner == "(|)" => {
                let mut items = Vec::new();
                for (_, branch) in branches {
                    items.push(self.dualize(*branch.first()?)?);
                }
                Adapter::Product { items, additive: true }
            }
            Adapter::Product { .. } | Adapter::Tagged { .. } => Adapter::Consumer { input: index },
            Adapter::Menu { owner, answers } => {
                let mut branches = Vec::new();
                for (label, answer) in answers {
                    branches.push((label, vec![self.dualize(answer)?]));
                }
                Adapter::Tagged { owner, branches }
            }
        };
        self.adapters[output] = adapter;
        Some(output)
    }

    fn components(&mut self, wanted: &[Type], given: &[Type]) -> Option<Vec<usize>> {
        wanted.iter().zip(given).map(|(wanted, given)| self.build(wanted, given, true)).collect()
    }

    fn instantiated_components(
        &mut self,
        fields: &[Type],
        wanted: &[Type],
        given: &[Type],
    ) -> Option<Vec<usize>> {
        self.components(
            &fields.iter().map(|field| field.instantiate(wanted)).collect::<Vec<_>>(),
            &fields.iter().map(|field| field.instantiate(given)).collect::<Vec<_>>(),
        )
    }
}

fn type_size(ty: &Type) -> usize {
    1 + match ty {
        Type::Tensor(items)
        | Type::Par(items)
        | Type::Sum(items)
        | Type::With(items)
        | Type::Named(_, items) => items.iter().map(type_size).sum(),
        Type::Dual(inner) | Type::Delayed(inner, _) | Type::Rowed(inner, _) => type_size(inner),
        _ => 0,
    }
}
