//! Function signatures: what a call site must supply, for declarations and
//! builtins alike, with template variables instantiated afresh per call.

use crate::declarations::Declarations;
use slc_core::types::{Base, Row, Type};
use slc_core::typing::Unification;
use slc_syntax::ast::{Decl, Program, TypeExpr};
use std::collections::HashMap;

#[derive(Debug)]
pub struct FunctionSignature {
    pub params: Vec<Type>,
    /// Which declared parameters are continuations, positionally. This is the
    /// declaration's continuation row.
    pub continuations: Vec<bool>,
    pub result: Option<Type>,
    /// What a call performs: the declared row, an operation's effect, or the
    /// `IO` a file primitive reaches. A row variable is a template, by the
    /// position of its type parameter, instantiated afresh at every call.
    pub row: Row,
    /// The name each parameter is declared under, positionally, for the
    /// diagnostic of an argument that does not fit it.
    pub param_names: Vec<String>,
    /// Trait bounds, as (type-parameter variable index, trait name): the
    /// signature uses `Type::Var(i)` for its i-th type parameter, so a bound
    /// `<T: Show>` on the 0th parameter is `(0, "Show")`.
    pub bounds: Vec<(usize, String)>,
    /// The polarity each type parameter declares, as (template variable
    /// index, parameter name, polarity): a call gives `<+T>` positive types
    /// only, and `<-T>` negative ones.
    pub signs: Vec<(usize, String, slc_syntax::ast::ParamPolarity)>,
    /// Whether this signature is the standard library's own. A builtin is
    /// applied by the runtime accumulating arguments, so it may be given
    /// fewer than all of them, and its template parameters take anything;
    /// the checker exempts it from what it holds a declaration to. Decided
    /// by where the signature came from, never by name — a program's own
    /// `fn add` is a declaration, and is checked as one.
    pub builtin: bool,
    pub nullary_value: bool,
}

/// The standard library.
///
/// A builtin whose outcome is a single value is an ordinary function. A
/// builtin whose outcome is not — it can fail, or find nothing — takes
/// continuations instead and denotes a command: the value arguments come
/// first, then one continuation per outcome, and exactly one is activated.
/// `continuations` marks which parameters are the continuation row.
struct Builtin {
    name: &'static str,
    params: Vec<Type>,
    continuations: Vec<bool>,
    result: Option<Type>,
}

fn builtin_functions() -> Vec<Builtin> {
    use Base::*;
    let i64 = Type::Pos(I64);
    let string = Type::Pos(Str);
    // The prelude's `enum Bool`.
    let bool_ = Type::Named("Bool".into(), Vec::new());
    let char_ = Type::Pos(Char);
    // Template variables: instantiated afresh at every call, so `same`
    // relates two slots of one call and promises nothing across calls.
    let same = Type::Var(0);

    // An ordinary function: every parameter is a value.
    let function = |name, params: Vec<Type>, result| Builtin {
        name,
        continuations: vec![false; params.len()],
        params,
        result,
    };
    // A command: `values` first, then a continuation per outcome.
    let offers = |name, values: Vec<Type>, outcomes: Vec<Type>| {
        let mut continuations = vec![false; values.len()];
        continuations.extend(std::iter::repeat_n(true, outcomes.len()));
        let mut params = values;
        params.extend(outcomes);
        Builtin { name, params, continuations, result: Some(Type::BOTTOM) }
    };

    vec![
        // A base value as text, beneath the prelude's `Display` impls.
        function("__display", vec![same.clone()], Some(string.clone())),
        // Arithmetic and comparison beneath the prelude's `Add`, `Eq`, `Ord`
        // and the rest, at whichever width or base type the impl gives.
        function("__neg", vec![same.clone()], Some(same.clone())),
        function("__add", vec![same.clone(), same.clone()], Some(same.clone())),
        function("__sub", vec![same.clone(), same.clone()], Some(same.clone())),
        function("__mul", vec![same.clone(), same.clone()], Some(same.clone())),
        function("__div", vec![same.clone(), same.clone()], Some(same.clone())),
        function("__rem", vec![same.clone(), same.clone()], Some(same.clone())),
        function("__eq", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("__ne", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("__lt", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("__gt", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("__le", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("__ge", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("str_len", vec![string.clone()], Some(i64.clone())),
        function("str_concat", vec![string.clone(), string.clone()], Some(string.clone())),
        function("int_to_str", vec![i64.clone()], Some(string.clone())),
        function("str_eq", vec![string.clone(), string.clone()], Some(bool_.clone())),
        function("is_digit", vec![char_.clone()], Some(bool_.clone())),
        function("is_ws", vec![char_.clone()], Some(bool_.clone())),
        function("skip_ws", vec![string.clone(), i64.clone()], Some(i64.clone())),
        function("skip_digits", vec![string.clone(), i64.clone()], Some(i64.clone())),
        function("substring", vec![string.clone(), i64.clone(), i64.clone()], Some(string.clone())),
        // The character at a position, beneath the prelude's `index`.
        function("__index", vec![string.clone(), i64.clone()], Some(char_.clone())),
        function("__file_exists", vec![string.clone()], Some(bool_.clone())),
        // Parsing, input/output, and lookup can fail or find nothing, so they
        // offer their outcomes to continuations.
        offers(
            "parse_int",
            vec![string.clone()],
            vec![Type::Neg(I64), Type::Neg(Str), Type::Neg(Str)],
        ),
        offers("__read_file", vec![string.clone()], vec![Type::Neg(Str), Type::Neg(Str)]),
        // A file handle: opened to one continuation, read line by line, and
        // spent by `__close_file` (`fs::close`).
        offers("__open_file", vec![string.clone()], vec![Type::Neg(File), Type::Neg(Str)]),
        offers("__read_line", vec![Type::Pos(File)], vec![Type::Neg(Str), Type::BOTTOM]),
        function("__close_file", vec![Type::Pos(File)], Some(Type::ONE)),
        offers(
            "__write_file",
            vec![string.clone(), string.clone()],
            vec![Type::BOTTOM, Type::Neg(Str)],
        ),
        offers("char_at", vec![string.clone(), i64.clone()], vec![Type::Neg(Char), Type::Neg(Str)]),
        offers("find_char", vec![string, i64.clone(), i64], vec![Type::Neg(I64), Type::Neg(Str)]),
    ]
}

/// The builtins that reach outside the program: the file primitives beneath
/// `fs` reach out directly, so calling one performs `IO`, exactly as a
/// written operation would.
pub(crate) fn builtin_effect(name: &str) -> Option<&'static str> {
    matches!(
        name,
        "__read_file"
            | "__write_file"
            | "__open_file"
            | "__read_line"
            | "__close_file"
            | "__file_exists"
    )
    .then_some(IO)
}

/// The one effect the runtime itself handles: `main` may leave it
/// undischarged, and nothing else may.
pub(crate) const IO: &str = "IO";

/// The name a parameter is declared under, or `_` for a pattern.
fn parameter_name(param: &slc_syntax::ast::Param) -> String {
    param.name().map(str::to_string).unwrap_or_else(|| "_".into())
}

/// A written type as a signature sees it: declaration names resolved, and a
/// generic name a template variable, instantiated afresh at every call. A
/// type that resolves to nothing gets its own template variable — unknown to
/// the caller, but one thing, not anything.
fn signature_type(
    ty: Option<&TypeExpr>,
    generics: &[String],
    enums: &Declarations,
    next_template: &mut usize,
) -> Type {
    // A generic name resolves to its positional template variable wherever
    // it stands — bare, under a sign, or inside a structured type such as
    // `(A -> B)` or `Stream<T>`: resolution maps it to a `Param`, the
    // parameters become the variables the caller instantiates, and a sign
    // is kept — `-T` is `dual(T)`, not `T` with the sign forgotten.
    let params: std::collections::HashMap<String, usize> =
        generics.iter().enumerate().map(|(i, g)| (g.clone(), i)).collect();
    let vars: Vec<Type> = (0..generics.len()).map(Type::Var).collect();
    ty.and_then(|ty| enums.resolve_in(ty, &params).map(|t| t.instantiate(&vars))).unwrap_or_else(
        || {
            let v = Type::Var(generics.len() + *next_template);
            *next_template += 1;
            v
        },
    )
}

pub(crate) fn function_types(
    p: &Program,
    enums: &Declarations,
) -> HashMap<String, FunctionSignature> {
    let mut out = builtin_functions()
        .into_iter()
        .map(|builtin| {
            let exit_row = builtin.continuations.iter().any(|is_exit| *is_exit).then_some(0);
            (
                builtin.name.to_string(),
                FunctionSignature {
                    row: Row {
                        effects: builtin_effect(builtin.name).map(Into::into).into_iter().collect(),
                        tail: exit_row,
                    },
                    param_names: Vec::new(),
                    params: builtin
                        .params
                        .into_iter()
                        .zip(&builtin.continuations)
                        .map(|(ty, is_exit)| {
                            if *is_exit {
                                let row = Row { effects: Default::default(), tail: exit_row };
                                Type::delayed(Type::rowed(ty, row.clone()), row)
                            } else {
                                ty
                            }
                        })
                        .collect(),
                    continuations: builtin.continuations,
                    result: builtin.result,
                    bounds: Vec::new(),
                    signs: Vec::new(),
                    builtin: true,
                    nullary_value: false,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    fn resolve_bounds(type_params: &[String], bounds: &[(String, String)]) -> Vec<(usize, String)> {
        bounds
            .iter()
            .filter_map(|(var, tr)| {
                type_params.iter().position(|p| p == var).map(|i| (i, tr.clone()))
            })
            .collect()
    }
    fn resolve_signs(
        type_params: &[String],
        signs: &[(String, slc_syntax::ast::ParamPolarity)],
    ) -> Vec<(usize, String, slc_syntax::ast::ParamPolarity)> {
        signs
            .iter()
            .filter_map(|(param, sign)| {
                type_params.iter().position(|p| p == param).map(|i| (i, param.clone(), *sign))
            })
            .collect()
    }
    for d in &p.decls {
        match &d.kind {
            Decl::Fn {
                name,
                params,
                return_type,
                polarity,
                type_params,
                type_param_signs,
                bounds,
                effects,
                ..
            } => {
                let mut next_template = 0;
                let resolved: Vec<Type> = params
                    .iter()
                    .map(|p| signature_type(p.ty.as_ref(), type_params, enums, &mut next_template))
                    .collect();
                // A negative function produces the *consumer* of the type
                // written after `<-`.
                let result =
                    signature_type(return_type.as_ref(), type_params, enums, &mut next_template);
                let result = if *polarity == slc_syntax::ast::FunctionPolarity::Negative {
                    result.dual()
                } else {
                    result
                };
                out.insert(
                    name.clone(),
                    FunctionSignature {
                        row: signature_row(effects, type_params, enums),
                        params: resolved,
                        param_names: params.iter().map(parameter_name).collect(),
                        continuations: params.iter().map(|p| p.is_continuation).collect(),
                        result: Some(result),
                        bounds: resolve_bounds(type_params, bounds),
                        signs: resolve_signs(type_params, type_param_signs),
                        builtin: false,
                        nullary_value: params.is_empty()
                            && *polarity == slc_syntax::ast::FunctionPolarity::Negative,
                    },
                );
            }
            Decl::Command {
                name,
                value_params,
                continuation_params,
                type_params,
                type_param_signs,
                bounds,
                effects,
                ..
            } => {
                let mut next_template = 0;
                let declared: Vec<_> =
                    value_params.iter().chain(continuation_params.iter()).collect();
                let params = declared
                    .iter()
                    .map(|p| signature_type(p.ty.as_ref(), type_params, enums, &mut next_template))
                    .collect();
                let continuations = declared.iter().map(|p| p.is_continuation).collect();
                out.insert(
                    name.clone(),
                    FunctionSignature {
                        row: signature_row(effects, type_params, enums),
                        params,
                        param_names: declared.iter().map(|p| parameter_name(p)).collect(),
                        continuations,
                        result: Some(Type::BOTTOM),
                        bounds: resolve_bounds(type_params, bounds),
                        signs: resolve_signs(type_params, type_param_signs),
                        builtin: false,
                        nullary_value: false,
                    },
                );
            }
            Decl::Effect { name: effect, operations, type_params, type_param_signs, .. } => {
                for op in operations {
                    let mut next_template = 0;
                    let params = op
                        .params
                        .iter()
                        .map(|p| {
                            signature_type(p.ty.as_ref(), type_params, enums, &mut next_template)
                        })
                        .collect();
                    let result = signature_type(
                        op.return_type.as_ref(),
                        type_params,
                        enums,
                        &mut next_template,
                    );
                    out.insert(
                        op.name.clone(),
                        FunctionSignature {
                            row: Row {
                                effects: [slc_core::types::Effect {
                                    name: effect.clone(),
                                    args: type_params
                                        .iter()
                                        .enumerate()
                                        .map(|(index, param)| {
                                            if type_param_signs
                                                .iter()
                                                .any(|(name, _)| name == param)
                                            {
                                                Type::Var(index)
                                            } else {
                                                Type::rowed(
                                                    Type::ONE,
                                                    Row {
                                                        effects: Default::default(),
                                                        tail: Some(index),
                                                    },
                                                )
                                            }
                                        })
                                        .collect(),
                                }]
                                .into(),
                                tail: None,
                            },
                            param_names: op.params.iter().map(parameter_name).collect(),
                            params,
                            continuations: op.params.iter().map(|_| false).collect(),
                            result: Some(result),
                            bounds: Vec::new(),
                            signs: resolve_signs(type_params, type_param_signs),
                            builtin: false,
                            nullary_value: false,
                        },
                    );
                }
            }
            _ => {}
        }
    }
    out
}

fn signature_row(row: &slc_syntax::ast::EffectRow, params: &[String], enums: &Declarations) -> Row {
    let scope = params.iter().enumerate().map(|(index, name)| (name.clone(), index)).collect();
    let variables: Vec<_> = (0..params.len()).map(Type::Var).collect();
    let resolved =
        enums.resolve_in(&TypeExpr::Row(row.clone()), &scope).map(|ty| ty.instantiate(&variables));
    match resolved {
        Some(Type::Rowed(_, row)) => row,
        _ => Row::default(),
    }
}

/// A signature's template variables are instantiated afresh at each call:
/// one template maps to one fresh variable within the call, and calls never
/// share them.
pub(crate) fn instantiate(
    signature: &FunctionSignature,
    uni: &mut Unification,
) -> (FunctionSignature, HashMap<usize, Type>) {
    let mut seen: HashMap<usize, Type> = HashMap::new();
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let fresh = FunctionSignature {
        params: signature.params.iter().map(|ty| freshen(ty, &mut seen, &mut rows, uni)).collect(),
        continuations: signature.continuations.clone(),
        result: signature.result.as_ref().map(|ty| freshen(ty, &mut seen, &mut rows, uni)),
        row: freshen_row(&signature.row, &mut seen, &mut rows, uni),
        param_names: signature.param_names.clone(),
        bounds: signature.bounds.clone(),
        signs: signature.signs.clone(),
        builtin: signature.builtin,
        nullary_value: signature.nullary_value,
    };
    (fresh, seen)
}

fn freshen(
    ty: &Type,
    seen: &mut HashMap<usize, Type>,
    rows: &mut HashMap<usize, usize>,
    uni: &mut Unification,
) -> Type {
    let mut each = |items: &[Type]| -> Vec<Type> {
        items.iter().map(|x| freshen(x, seen, rows, uni)).collect()
    };
    match ty {
        Type::Var(v) => seen.entry(*v).or_insert_with(|| uni.fresh_var()).clone(),
        Type::Tensor(items) => Type::Tensor(each(items)),
        Type::Par(items) => Type::Par(each(items)),
        Type::With(items) => Type::With(each(items)),
        Type::Sum(items) => Type::Sum(each(items)),
        Type::Dual(t) => Type::Dual(Box::new(freshen(t, seen, rows, uni))),
        Type::Named(name, args) => Type::Named(name.clone(), each(args)),
        Type::Rowed(t, row) => {
            Type::Rowed(Box::new(freshen(t, seen, rows, uni)), freshen_row(row, seen, rows, uni))
        }
        Type::Delayed(inner, row) => {
            Type::delayed(freshen(inner, seen, rows, uni), freshen_row(row, seen, rows, uni))
        }
        atom => atom.clone(),
    }
}

/// A signature's row with its template row variable instantiated: one
/// template, one fresh row variable per call.
fn freshen_row(
    row: &Row,
    seen: &mut HashMap<usize, Type>,
    rows: &mut HashMap<usize, usize>,
    uni: &mut Unification,
) -> Row {
    let mut fresh = row.map_types(|argument| freshen(argument, seen, rows, uni));
    fresh.tail = row.tail.map(|template| *rows.entry(template).or_insert_with(|| uni.fresh_row()));
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_core::types::Effect;

    #[test]
    fn signature_instantiation_shares_effect_arguments_with_value_types() {
        let nested = Row {
            effects: [Effect { name: "Nested".into(), args: vec![Type::Var(1)] }].into(),
            tail: Some(3),
        };
        let signature = FunctionSignature {
            params: vec![Type::Var(0)],
            result: Some(Type::Var(0)),
            row: Row {
                effects: [Effect {
                    name: "Reader".into(),
                    args: vec![Type::Var(0), Type::rowed(Type::ONE, nested)],
                }]
                .into(),
                tail: Some(3),
            },
            continuations: vec![false],
            param_names: vec!["value".into()],
            bounds: Vec::new(),
            signs: Vec::new(),
            builtin: false,
            nullary_value: false,
        };
        let mut uni = Unification::new();
        let (first, first_variables) = instantiate(&signature, &mut uni);
        let (second, second_variables) = instantiate(&signature, &mut uni);
        assert_ne!(first_variables[&0], second_variables[&0]);
        assert_ne!(first_variables[&1], second_variables[&1]);
        assert_ne!(first.row.tail, second.row.tail);
        for (instance, variables) in [(first, first_variables), (second, second_variables)] {
            let effect = instance.row.effects.iter().next().unwrap();
            assert_eq!(effect.args[0], instance.params[0]);
            assert_eq!(instance.result, Some(instance.params[0].clone()));
            let Type::Rowed(_, nested) = &effect.args[1] else { panic!("missing nested row") };
            assert_eq!(nested.tail, instance.row.tail);
            assert_eq!(nested.effects.iter().next().unwrap().args, vec![variables[&1].clone()]);
        }
    }
}
