//! Function signatures: what a call site must supply, for declarations and
//! builtins alike, with template variables instantiated afresh per call.

use crate::declarations::Declarations;
use slc_core::types::{Base, Type};
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
    let bool_ = Type::Pos(Bool);
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
        function("add", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("sub", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("mul", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("div", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("rem", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("eq", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("ne", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("lt", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("gt", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("le", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("ge", vec![same.clone(), same.clone()], Some(bool_.clone())),
        function("str_len", vec![string.clone()], Some(i64.clone())),
        function("str_concat", vec![string.clone(), string.clone()], Some(string.clone())),
        function("int_to_str", vec![i64.clone()], Some(string.clone())),
        function("str_eq", vec![string.clone(), string.clone()], Some(bool_.clone())),
        function("is_digit", vec![char_.clone()], Some(bool_.clone())),
        function("is_ws", vec![char_.clone()], Some(bool_.clone())),
        function("skip_ws", vec![string.clone(), i64.clone()], Some(i64.clone())),
        function("skip_digits", vec![string.clone(), i64.clone()], Some(i64.clone())),
        function("substring", vec![string.clone(), i64.clone(), i64.clone()], Some(string.clone())),
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
        offers("__read_line", vec![Type::Pos(File)], vec![Type::Neg(Str), Type::Neg(Unit)]),
        function("__close_file", vec![Type::Pos(File)], Some(Type::ONE)),
        offers(
            "__write_file",
            vec![string.clone(), string.clone()],
            vec![Type::Neg(Unit), Type::Neg(Str)],
        ),
        offers("char_at", vec![string.clone(), i64.clone()], vec![Type::Neg(Char), Type::Neg(Str)]),
        offers("find_char", vec![string, i64.clone(), i64], vec![Type::Neg(I64), Type::Neg(Str)]),
    ]
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
            (
                builtin.name.to_string(),
                FunctionSignature {
                    params: builtin.params,
                    continuations: builtin.continuations,
                    result: builtin.result,
                    bounds: Vec::new(),
                    signs: Vec::new(),
                    builtin: true,
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
                        params: resolved,
                        continuations: params.iter().map(|p| p.is_continuation).collect(),
                        result: Some(result),
                        bounds: resolve_bounds(type_params, bounds),
                        signs: resolve_signs(type_params, type_param_signs),
                        builtin: false,
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
                        params,
                        continuations,
                        result: Some(Type::BOTTOM),
                        bounds: resolve_bounds(type_params, bounds),
                        signs: resolve_signs(type_params, type_param_signs),
                        builtin: false,
                    },
                );
            }
            Decl::Effect { operations, .. } => {
                for op in operations {
                    let mut next_template = 0;
                    let params = op
                        .params
                        .iter()
                        .map(|p| signature_type(p.ty.as_ref(), &[], enums, &mut next_template))
                        .collect();
                    let result =
                        signature_type(op.return_type.as_ref(), &[], enums, &mut next_template);
                    out.insert(
                        op.name.clone(),
                        FunctionSignature {
                            params,
                            continuations: op.params.iter().map(|_| false).collect(),
                            result: Some(result),
                            bounds: Vec::new(),
                            signs: Vec::new(),
                            builtin: false,
                        },
                    );
                }
            }
            _ => {}
        }
    }
    out
}

/// A signature's template variables are instantiated afresh at each call:
/// one template maps to one fresh variable within the call, and calls never
/// share them.
pub(crate) fn instantiate(
    signature: &FunctionSignature,
    uni: &mut Unification,
) -> (FunctionSignature, HashMap<usize, Type>) {
    let mut seen: HashMap<usize, Type> = HashMap::new();
    let fresh = FunctionSignature {
        params: signature.params.iter().map(|ty| freshen(ty, &mut seen, uni)).collect(),
        continuations: signature.continuations.clone(),
        result: signature.result.as_ref().map(|ty| freshen(ty, &mut seen, uni)),
        bounds: signature.bounds.clone(),
        signs: signature.signs.clone(),
        builtin: signature.builtin,
    };
    (fresh, seen)
}

fn freshen(ty: &Type, seen: &mut HashMap<usize, Type>, uni: &mut Unification) -> Type {
    match ty {
        Type::Var(v) => seen.entry(*v).or_insert_with(|| uni.fresh_var()).clone(),
        Type::Tensor(items) => Type::Tensor(items.iter().map(|x| freshen(x, seen, uni)).collect()),
        Type::Par(items) => Type::Par(items.iter().map(|x| freshen(x, seen, uni)).collect()),
        Type::With(items) => Type::With(items.iter().map(|x| freshen(x, seen, uni)).collect()),
        Type::Sum(items) => Type::Sum(items.iter().map(|x| freshen(x, seen, uni)).collect()),
        Type::Dual(t) => Type::Dual(Box::new(freshen(t, seen, uni))),
        Type::Named(name, args) => {
            Type::Named(name.clone(), args.iter().map(|a| freshen(a, seen, uni)).collect())
        }
        atom => atom.clone(),
    }
}
