//! Lowering from surface AST to λ̄μμ̃ core IR.

use crate::ast::*;
use crate::token::Span;
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::term::{CoMatchBranch, Term};
use slc_core::types::{Base, Type};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CONSTANTS: RefCell<HashMap<String, Pattern>> = RefCell::new(HashMap::new());
    /// Variant name → fully qualified label, for every declared enum. An
    /// unqualified variant name is recorded only when it is unambiguous.
    static VARIANTS: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
    /// Trait-method call span → how the checker resolved it. A concrete
    /// receiver is a direct call to the impl; a bounded one projects the
    /// method from the enclosing function's dictionary parameter.
    static METHODS: RefCell<HashMap<Span, MethodDispatch>> = RefCell::new(HashMap::new());
    /// Call-to-bounded-function span → the dictionary arguments to pass
    /// (variable names), in the order the function's bounds are declared.
    static CALLS: RefCell<HashMap<Span, Vec<String>>> = RefCell::new(HashMap::new());
    /// Projection span → the component index the checker resolved (`.i`, or a
    /// record field's position).
    static PROJECTIONS: RefCell<HashMap<Span, usize>> = RefCell::new(HashMap::new());
    /// Destructor name → fully qualified label, for every declared menu. An
    /// unqualified destructor name is recorded only when it is unambiguous.
    static MENUS: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
    /// Demand span → the qualified destructor label the checker resolved
    /// (`cfg.item` on a menu, as opposed to a struct projection).
    static DEMANDS: RefCell<HashMap<Span, String>> = RefCell::new(HashMap::new());
}

/// How a trait-method call dispatches, as the checker resolved it.
#[derive(Debug, Clone)]
pub enum MethodDispatch {
    /// The receiver type is concrete: call the impl directly.
    Static(String),
    /// The receiver is a bound type parameter: project method `index` (of
    /// `count` the trait declares) from the dictionary named `dict_var`.
    Dict { dict_var: String, index: usize, count: usize },
}

/// What the checker resolved about a program's trait dispatch, handed to
/// lowering so method calls become direct calls or dictionary projections
/// and bounded functions take and forward dictionaries.
#[derive(Debug, Clone, Default)]
pub struct DispatchInfo {
    pub methods: HashMap<Span, MethodDispatch>,
    pub calls: HashMap<Span, Vec<String>>,
    /// Projection span → the resolved component index (`.i`, or a struct
    /// field's position).
    pub projections: HashMap<Span, usize>,
    /// Demand span → the qualified destructor label: `cfg.item` resolved
    /// against a `menu` declaration rather than a struct's fields.
    pub demands: HashMap<Span, String>,
}

/// The dictionary parameter name for a bound: one value threaded into a
/// bounded function, carrying the trait's impls for that type parameter.
pub fn dict_param_name(trait_name: &str, type_param: &str) -> String {
    format!("__dict_{trait_name}_{type_param}")
}

/// The global dictionary name for a concrete `(trait, type key)`.
pub fn dict_global_name(trait_name: &str, key: &str) -> String {
    format!("__dict_{trait_name}_{key}")
}

/// Project method `index` from a dictionary. A single-method trait's
/// dictionary is its one impl, so the dictionary *is* the method; a
/// multi-method dictionary is a right-nested tuple of impls, projected by
/// binding all `count` components and returning the `index`-th.
fn dict_projection(dict_var: &str, index: usize, count: usize) -> Term {
    if count <= 1 {
        return Term::Var(dict_var.to_string());
    }
    let binders: Vec<String> = (0..count).map(|i| format!("__d{i}")).collect();
    let chosen = binders[index].clone();
    Term::Mu(
        "__dp".into(),
        Box::new(Command::Cut(
            Term::Var(dict_var.to_string()),
            CoTerm::MuTildeTensor(
                binders,
                Box::new(Command::Cut(Term::Var(chosen), CoTerm::Covar("__dp".into()))),
            ),
        )),
    )
}

/// Wrap a bounded declaration's body in its dictionary parameters, outermost
/// and in declared-bound order, so a call supplies them before the value
/// arguments.
fn bind_dict_params(bounds: &[(String, String)], mut term: Term) -> Term {
    for (type_param, trait_name) in bounds.iter().rev() {
        term = Term::Lam(dict_param_name(trait_name, type_param), Box::new(term));
    }
    term
}

/// The fully qualified label a variant path or unambiguous variant name
/// denotes, if it names a declared enum variant.
fn lookup_variant(name: &str) -> Option<String> {
    VARIANTS.with(|cell| cell.borrow().get(name).cloned().flatten())
}

/// How the checker resolved the trait-method call at `span`, if it is one.
fn method_dispatch(span: Span) -> Option<MethodDispatch> {
    METHODS.with(|cell| cell.borrow().get(&span).cloned())
}

/// The dictionary arguments a call at `span` must pass, if it calls a
/// bounded function.
fn call_dicts(span: Span) -> Option<Vec<String>> {
    CALLS.with(|cell| cell.borrow().get(&span).cloned())
}

/// The component index the checker resolved for a projection at `span`.
fn projection(span: Span) -> Option<usize> {
    PROJECTIONS.with(|cell| cell.borrow().get(&span).copied())
}

/// The qualified destructor label a destructor name denotes, if it names a
/// declared menu item unambiguously (or is already qualified).
fn lookup_dtor(name: &str) -> Option<String> {
    MENUS.with(|cell| cell.borrow().get(name).cloned().flatten())
}

/// The qualified destructor label the checker resolved for the demand at
/// `span`, if `base.item` demands a menu rather than projecting a struct.
fn demand(span: Span) -> Option<String> {
    DEMANDS.with(|cell| cell.borrow().get(&span).cloned())
}

/// Lower a program with the checker's dispatch resolution in force, so that
/// trait-method calls become direct calls or dictionary projections and
/// bounded functions take and forward their dictionaries.
pub fn lower_program_resolving(
    p: &Program,
    dispatch: &DispatchInfo,
) -> Result<Vec<(String, Term)>, LowerError> {
    METHODS.with(|cell| *cell.borrow_mut() = dispatch.methods.clone());
    CALLS.with(|cell| *cell.borrow_mut() = dispatch.calls.clone());
    PROJECTIONS.with(|cell| *cell.borrow_mut() = dispatch.projections.clone());
    DEMANDS.with(|cell| *cell.borrow_mut() = dispatch.demands.clone());
    let result = lower_program(p);
    METHODS.with(|cell| cell.borrow_mut().clear());
    CALLS.with(|cell| cell.borrow_mut().clear());
    PROJECTIONS.with(|cell| cell.borrow_mut().clear());
    DEMANDS.with(|cell| cell.borrow_mut().clear());
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
    UnknownType(String),
    Unsupported(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::UnknownType(s) => write!(f, "unknown type: {s}"),
            LowerError::Unsupported(s) => write!(f, "unsupported construct: {s}"),
        }
    }
}

impl std::error::Error for LowerError {}

pub fn lower_type(t: &TypeExpr) -> Result<Type, LowerError> {
    match t {
        TypeExpr::Base(s) => match s.as_str() {
            "i32" => Ok(Type::Pos(Base::I32)),
            "i64" => Ok(Type::Pos(Base::I64)),
            "u32" => Ok(Type::Pos(Base::U32)),
            "u64" => Ok(Type::Pos(Base::U64)),
            "bool" => Ok(Type::Pos(Base::Bool)),
            "String" | "str" => Ok(Type::Pos(Base::Str)),
            "char" => Ok(Type::Pos(Base::Char)),
            "unit" => Ok(Type::Pos(Base::Unit)),
            "File" => Ok(Type::Pos(Base::File)),
            other => Err(LowerError::UnknownType(other.to_string())),
        },
        // `-⊥` is not the dual of `⊥`; bottom is the impossible command
        // type. It is negative already.
        TypeExpr::Negative(inner) if matches!(inner.kind, TypeExpr::Bottom) => Ok(Type::Bottom),
        TypeExpr::Positive(inner) => Ok(lower_type(&inner.kind)?.dual().dual()),
        TypeExpr::Negative(inner) => Ok(lower_type(&inner.kind)?.dual()),
        TypeExpr::Tensor(a, b) => {
            Ok(Type::Tensor(Box::new(lower_type(&a.kind)?), Box::new(lower_type(&b.kind)?)))
        }
        TypeExpr::Par(a, b) => {
            Ok(Type::Par(Box::new(lower_type(&a.kind)?), Box::new(lower_type(&b.kind)?)))
        }
        // `A → B` is `-A ⅋ B`, so a function is negative and `A → ⊥` is
        // `-A`: a function that never returns is a consumer of its argument.
        TypeExpr::Fun(a, b) => Ok(Type::arrow(lower_type(&a.kind)?, lower_type(&b.kind)?)),
        TypeExpr::List(inner) => Ok(Type::List(Box::new(lower_type(&inner.kind)?))),
        // `dual(A)` applies the involution rather than wrapping a node, so
        // `dual(+i64)` is `-i64` and `dual(dual(A))` is `A`. Only a
        // declaration's name stays wrapped: it is opaque to the core.
        TypeExpr::Dual(inner) => Ok(lower_type(&inner.kind)?.dual()),
        TypeExpr::Down(inner) => Ok(Type::Down(Box::new(lower_type(&inner.kind)?))),
        TypeExpr::Up(inner) => Ok(Type::Up(Box::new(lower_type(&inner.kind)?))),
        TypeExpr::Unit => Ok(Type::One),
        TypeExpr::Bottom => Ok(Type::Bottom),
    }
}

/// Lower an expression to a core term.
/// Lower an expression in an explicit lexical continuation scope. New
/// continuation binders extend `continuations` for their body only.
fn lower_expr(e: &Node<Expr>, continuations: &[String]) -> Result<Term, LowerError> {
    match &e.kind {
        Expr::Int(n) => Ok(Term::Var(format!("$int_{n}"))),
        Expr::Float(n) => Ok(Term::Var(format!("$float_{n}"))),
        Expr::Str(s) => Ok(Term::Var(format!("$str_{s:?}"))),
        Expr::Char(c) => Ok(Term::Var(format!("$char_{c}"))),
        Expr::Bool(b) => Ok(Term::Var(if *b { "true" } else { "false" }.to_string())),
        Expr::Ident(s) => {
            // A variant path resolves to the global the enum declaration
            // installs; any other identifier stays a variable.
            Ok(Term::Var(s.clone()))
        }

        Expr::Lambda { param, param_type: _, return_type: _, body } => {
            // fn(x) { body } → λx. body'. An ordinary lambda opens a fresh
            // lexical continuation scope: only its own local mu/select
            // binders are visible in its body.
            let b = lower_expr(body, &[])?;
            Ok(Term::Lam(param.clone(), Box::new(b)))
        }

        Expr::Call { callee, args } => {
            // An enum constructor is not an application: `E::V(a)` is the
            // labelled injection `E::V(a)`. Several payload values are packed
            // into one right-nested tensor, so a variant always carries
            // exactly one payload term.
            if let Expr::Ident(name) = &callee.kind
                && let Some(label) = lookup_variant(name)
            {
                let mut payload = Term::Var("$unit".into());
                for arg in args.iter().rev() {
                    let value = lower_expr(arg, continuations)?;
                    payload = if payload == Term::Var("$unit".into()) {
                        value
                    } else {
                        Term::Pair(Box::new(value), Box::new(payload))
                    };
                }
                return Ok(Term::Tag(label, Box::new(payload)));
            }
            // f(a, b) lowers to nested single-argument applications:
            //   f(a) applied to (b)
            // Multi-arg functions are curried: fn f(x, y) → λx. λy. body.
            // A trait-method call resolves to a direct impl call (concrete
            // receiver) or a projection from a dictionary parameter (bounded
            // receiver) — never a runtime method value.
            let mut result = match &callee.kind {
                Expr::Ident(_) => match method_dispatch(e.span) {
                    Some(MethodDispatch::Static(mangled)) => Term::Var(mangled),
                    Some(MethodDispatch::Dict { dict_var, index, count }) => {
                        dict_projection(&dict_var, index, count)
                    }
                    None => lower_expr(callee, continuations)?,
                },
                _ => lower_expr(callee, continuations)?,
            };
            // A call to a bounded function forwards its dictionaries first,
            // in bound order, then the value arguments.
            let mut call_args: Vec<Term> =
                call_dicts(e.span).unwrap_or_default().into_iter().map(Term::Var).collect();
            if args.is_empty() {
                // A call with no value arguments still applies its callee, to
                // the marker that carries none.
                call_args.push(Term::Var(NO_ARGUMENTS.into()));
            } else {
                for arg in args {
                    call_args.push(lower_expr(arg, continuations)?);
                }
            }
            for arg in call_args {
                result = Term::Mu(
                    "__call".into(),
                    Box::new(Command::Cut(
                        result,
                        CoTerm::App(arg, Box::new(CoTerm::Covar("__call".into()))),
                    )),
                );
            }
            Ok(result)
        }

        Expr::Pair(items) => {
            // (e1, e2) → e1' ⊗ e2'
            let mut terms: Vec<Term> = Vec::new();
            for item in items {
                terms.push(lower_expr(item, continuations)?);
            }
            match terms.len() {
                0 => Ok(Term::Var("$unit".into())),
                1 => Ok(terms.pop().unwrap()),
                _ => {
                    // Right-fold into nested pairs
                    let mut it = terms.into_iter().rev();
                    let mut acc = it.next().unwrap();
                    for t in it {
                        acc = Term::Pair(Box::new(t), Box::new(acc));
                    }
                    Ok(acc)
                }
            }
        }

        Expr::Let { name, value, body, .. } => {
            let v = lower_expr(value, continuations)?;
            let b = body
                .as_ref()
                .map(|b| lower_expr(b, continuations))
                .transpose()?
                .unwrap_or_else(|| Term::Var("$unit".into()));
            Ok(lower_let(name, v, b))
        }

        Expr::If { cond, then, otherwise } => {
            // Branches are wrapped in λ so they are only evaluated when
            // chosen — if must be lazy, or mu escapes in the untaken branch
            // would fire eagerly.
            let c = lower_expr(cond, continuations)?;
            let t = Term::Lam("__unused".into(), Box::new(lower_expr(then, continuations)?));
            let e = otherwise
                .as_ref()
                .map(|e| {
                    lower_expr(e, continuations).map(|b| Term::Lam("__unused".into(), Box::new(b)))
                })
                .transpose()?
                .unwrap_or_else(|| {
                    Term::Lam("__unused".into(), Box::new(Term::Var("$unit".into())))
                });
            // Build: μif. ⟨ cond ∥ μ̃__cond. ⟨ μ__call. ⟨ __if_dispatch ∥ (…) · __call ⟩ ∥ __tail ⟩ ⟩
            // The dispatch builtin applies the chosen thunk to unit.
            let triple = Term::Pair(
                Box::new(Term::Var("__cond".into())),
                Box::new(Term::Pair(Box::new(t), Box::new(e))),
            );
            let dispatch_call = Term::Mu(
                "__call".into(),
                Box::new(Command::Cut(
                    Term::Var("__if_dispatch".into()),
                    CoTerm::App(triple, Box::new(CoTerm::Covar("__call".into()))),
                )),
            );
            Ok(Term::Mu(
                "__if".into(),
                Box::new(Command::Cut(
                    c,
                    CoTerm::MuTilde(
                        "__cond".into(),
                        Box::new(Command::Cut(dispatch_call, CoTerm::Covar("__tail".into()))),
                    ),
                )),
            ))
        }

        Expr::BinOp { op, lhs, rhs } => {
            if matches!(op, BinOp::And | BinOp::Or)
                && let Some(expanded) = lower_boolean_operator(op, lhs, rhs, e.span)
            {
                return lower_expr(&expanded, continuations);
            }
            let name = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::Mul => "mul",
                BinOp::Div => "div",
                BinOp::Mod => "rem",
                BinOp::Eq => "eq",
                BinOp::Ne => "ne",
                BinOp::Lt => "lt",
                BinOp::Gt => "gt",
                BinOp::Le => "le",
                BinOp::Ge => "ge",
                BinOp::And | BinOp::Or => unreachable!("boolean operators expand before lowering"),
            };
            Ok(call_curried(
                Term::Var(name.into()),
                vec![lower_expr(lhs, continuations)?, lower_expr(rhs, continuations)?],
            ))
        }

        Expr::UnOp { op, body } => {
            if matches!(op, UnOp::Not) {
                let arg = lower_expr(body, continuations)?;
                return Ok(call_curried(
                    Term::Var("eq".into()),
                    vec![arg, Term::Var("false".into())],
                ));
            }
            Ok(call_curried(Term::Var("neg".into()), vec![lower_expr(body, continuations)?]))
        }

        Expr::Index { value, index } => Ok(call_curried(
            Term::Var("__index".into()),
            vec![lower_expr(value, continuations)?, lower_expr(index, continuations)?],
        )),

        Expr::Slice { value, start, end } => {
            let start_term = start
                .as_ref()
                .map(|e| lower_expr(e, continuations))
                .transpose()?
                .unwrap_or_else(|| Term::Var("$int_0".into()));
            let end_term = end
                .as_ref()
                .map(|e| lower_expr(e, continuations))
                .transpose()?
                .unwrap_or_else(|| Term::Var("__string_len".into()));
            Ok(call_curried(
                Term::Var("substring".into()),
                vec![lower_expr(value, continuations)?, start_term, end_term],
            ))
        }

        Expr::Cut { value, consumer } => {
            // `v @ k` is the cut ⟨v ∥ k⟩: a command, not an application. It
            // is wrapped in a μ binder that its body never mentions, because
            // a command has no result and control does not return from it.
            let v = lower_expr(value, continuations)?;
            let command = match named_consumer(&consumer.kind) {
                // A named consumer is a co-variable, so the cut is direct.
                Some(name) => Command::Cut(v, CoTerm::Covar(name.clone())),
                // Any other consumer is an expression that produces one:
                // evaluate it, then apply it to the value — the same shape
                // as an application, ⟨ ⟦k⟧ ∥ ⟦v⟧ · __tail ⟩.
                _ => Command::Cut(
                    lower_expr(consumer, continuations)?,
                    CoTerm::App(v, Box::new(CoTerm::Covar("__tail".into()))),
                ),
            };
            Ok(Term::Mu(cut_binder(&consumer.kind), Box::new(command)))
        }
        Expr::Mu { continuation_params, body, .. } => {
            let mut body_scope = continuations.to_vec();
            body_scope.extend(continuation_params.iter().map(|p| p.name.clone()));
            let mut term = lower_expr(body, &body_scope)?;
            for p in continuation_params.iter().rev() {
                term = Term::Mu(
                    p.name.clone(),
                    Box::new(Command::Cut(term, CoTerm::Covar(p.name.clone()))),
                );
            }
            Ok(term)
        }
        // A shift is a coercion the checker cares about and the core does
        // not: a boxed consumer and the consumer are the same value.
        Expr::Shift { expr, .. } => lower_expr(expr, continuations),
        // `base.i` / `base.field` → μ. ⟨ ⟦base⟧ ∥ prj:index ⟩. The checker
        // resolved the component index from the base's type; the μ binder is
        // vestigial — the projected component returns to the ambient
        // continuation, as a match dispatch does.
        Expr::Project { base, .. } => {
            // `cfg.item` on a menu is a demand: cut the menu against the
            // request, with the μ binder as the answer's continuation.
            if let Some(label) = demand(e.span) {
                let base = lower_expr(base, continuations)?;
                return Ok(Term::Mu(
                    "__ask".into(),
                    Box::new(Command::Cut(
                        base,
                        CoTerm::Dtor(label, Box::new(CoTerm::Covar("__ask".into()))),
                    )),
                ));
            }
            let index = projection(e.span).ok_or_else(|| {
                LowerError::Unsupported("a projection was not resolved by the checker".into())
            })?;
            let base = lower_expr(base, continuations)?;
            Ok(Term::Mu("__prj".into(), Box::new(Command::Cut(base, CoTerm::Prj(index)))))
        }
        // `.item(k)` — a request literal: the continuation boxed under its
        // destructor. A named continuation is a co-variable directly; any
        // other expression is bound first, then named.
        Expr::Request { dtor, arg } => {
            let label = lookup_dtor(dtor).ok_or_else(|| {
                LowerError::Unsupported(format!("`.{dtor}` does not name a declared menu item"))
            })?;
            if let Expr::Ident(name) = &arg.kind {
                return Ok(Term::Co(Box::new(CoTerm::Dtor(
                    label,
                    Box::new(CoTerm::Covar(name.clone())),
                ))));
            }
            let payload = lower_expr(arg, continuations)?;
            Ok(Term::Mu(
                "__req".into(),
                Box::new(Command::Cut(
                    payload,
                    CoTerm::MuTilde(
                        "__k".into(),
                        Box::new(Command::Cut(
                            Term::Co(Box::new(CoTerm::Dtor(
                                label,
                                Box::new(CoTerm::Covar("__k".into())),
                            ))),
                            CoTerm::Covar("__req".into()),
                        )),
                    ),
                )),
            ))
        }
        // `handle` lowers to a `__handle` call the runtime special-cases: the
        // effect name, a value encoding the clauses, and a thunk of the body.
        Expr::Handle { body, clauses, ret } => {
            // Each clause → ($str_op ⊗ λarg. λresume. body); the arg binds the
            // operation's single parameter (or is ignored for a nullary op).
            let mut encoded = Term::Var("$unit".into());
            for clause in clauses.iter().rev() {
                let mut body_scope = continuations.to_vec();
                body_scope.push(clause.resume.clone());
                body_scope.extend(clause.params.iter().cloned());
                let inner = lower_expr(&clause.body, &body_scope)?;
                let arg_binder =
                    clause.params.first().cloned().unwrap_or_else(|| "__op_arg".into());
                let closure = Term::Lam(
                    arg_binder,
                    Box::new(Term::Lam(clause.resume.clone(), Box::new(inner))),
                );
                let pair = Term::Pair(
                    Box::new(Term::Var(format!("$str_\"{}\"", clause.op))),
                    Box::new(closure),
                );
                encoded = Term::Pair(Box::new(pair), Box::new(encoded));
            }
            // The return clause, or the identity.
            let ret_closure = match ret {
                Some((binder, rbody)) => {
                    Term::Lam(binder.clone(), Box::new(lower_expr(rbody, continuations)?))
                }
                None => Term::Lam("__ret".into(), Box::new(Term::Var("__ret".into()))),
            };
            let ret_pair =
                Term::Pair(Box::new(Term::Var("$str_\"return\"".into())), Box::new(ret_closure));
            encoded = Term::Pair(Box::new(ret_pair), Box::new(encoded));
            let body_thunk =
                Term::Lam("__handle_thunk".into(), Box::new(lower_expr(body, continuations)?));
            // The clause tree is wrapped so the runtime's argument collection,
            // which flattens pairs, passes it as one value.
            Ok(call_curried(
                Term::Var("__handle".into()),
                vec![Term::Tag("__clauses".into(), Box::new(encoded)), body_thunk],
            ))
        }
        Expr::Match { scrutinee, arms } => {
            // match s { p1 => e1, p2 => e2, ... }
            // → μmatch. ⟨ __match_dispatch(s', arms...) ∥ match ⟩
            // The dispatch builtin evaluates the scrutinee and selects
            // the arm whose pattern matches, applying it as a thunk.
            let s = lower_expr(scrutinee, continuations)?;
            // Encode arms as thunks: one closure per arm.
            // Each arm is a Lam so it is only evaluated when selected.
            let mut arm_terms = Vec::new();
            for arm in arms {
                let descriptor = Term::Var(format!("$str_{}", pattern_descriptor(&arm.pattern)));
                let b = lower_expr(&arm.body, continuations)?;
                let guard = match &arm.guard {
                    Some(guard) => lower_expr(guard, continuations)?,
                    None => Term::Var("true".into()),
                };
                arm_terms.push(Term::Tag(
                    "__match_arm".into(),
                    Box::new(Term::Pair(
                        Box::new(descriptor),
                        Box::new(Term::Pair(
                            Box::new(guard),
                            Box::new(Term::Lam("__match_arg".into(), Box::new(b))),
                        )),
                    )),
                ));
            }
            // Keep the arm spine right-nested: (s, (a1, (a2, ...))).
            // `__match_dispatch` walks this spine, so left-nesting would
            // accidentally make the first arm part of the scrutinee.
            let mut spine = Term::Var("$unit".into());
            for a in arm_terms.into_iter().rev() {
                spine = Term::Pair(Box::new(a), Box::new(spine));
            }
            let payload = Term::Pair(Box::new(s), Box::new(spine));
            Ok(Term::Mu(
                "__match".into(),
                Box::new(Command::Cut(
                    Term::Var("__match_dispatch".into()),
                    CoTerm::App(payload, Box::new(CoTerm::Covar("__match".into()))),
                )),
            ))
        }

        Expr::Data { name, fields } => {
            // A record value is a labelled product: the declaration's name
            // tags the right-nested tensor of its field values. An `enum`
            // variant is the same shape with a different label, so one core
            // form covers both.
            let mut payload = Term::Var("$unit".into());
            for (index, (_, value)) in fields.iter().enumerate().rev() {
                let value = lower_expr(value, continuations)?;
                payload = if index + 1 == fields.len() {
                    value
                } else {
                    Term::Pair(Box::new(value), Box::new(payload))
                };
            }
            Ok(Term::Tag(name.clone(), Box::new(payload)))
        }

        // `mu T { .item(k) <= c, … }` — the copattern form: a menu value,
        // μ[…], one branch per demand.
        Expr::CoMatch { ty, arms } => {
            let mut branches = Vec::new();
            for arm in arms {
                let Pattern::Dtor { dtor, binder } = &arm.pattern else {
                    return Err(LowerError::Unsupported(
                        "`mu` with arms answers a menu's demands; every arm is `.item(k)`".into(),
                    ));
                };
                // Qualify against the written menu first, then the
                // unambiguous-destructor table.
                let label = ty
                    .as_ref()
                    .and_then(|ty| match &ty.kind {
                        TypeExpr::Base(name) => lookup_dtor(&format!("{name}::{dtor}")),
                        _ => None,
                    })
                    .or_else(|| lookup_dtor(dtor))
                    .ok_or_else(|| {
                        LowerError::Unsupported(format!(
                            "`.{dtor}` does not name a declared menu item"
                        ))
                    })?;
                let body = lower_select_command(&arm.command, continuations)?;
                branches.push(CoMatchBranch {
                    label,
                    binder: binder.clone(),
                    body: Box::new(body),
                });
            }
            Ok(Term::CoMatch(branches))
        }

        Expr::Select { arms, .. } => {
            // `select T { p <= c, … }` is the consumer of T, given by cases
            // on it: one branch per shape, binding that shape's components.
            //
            //   labelled (enum, struct) ⟹ co(μ̃[ L(x…). c | … ])
            //   product (tensor)        ⟹ co(μ̃(x…). c)
            // Request arms belong to `mu`: `select` answers data.
            if arms.iter().any(|arm| matches!(arm.pattern, Pattern::Dtor { .. })) {
                return Err(LowerError::Unsupported(
                    "`select` answers data; a menu answers demands and is built by \
                     `mu Menu { .item(k) <= c, … }`"
                        .into(),
                ));
            }
            let mut branches = Vec::new();
            let mut product: Option<(Vec<String>, Command)> = None;
            for arm in arms {
                let (label, binders) = select_arm_shape(&arm.pattern)?;
                let command = lower_select_command(&arm.command, continuations)?;
                match label {
                    Some(label) => {
                        branches.push(CoCaseBranch { label, binders, body: Box::new(command) })
                    }
                    None => product = Some((binders, command)),
                }
            }
            match (branches.is_empty(), product) {
                (true, Some((binders, command))) => {
                    // One binder takes the whole value: that is `μ̃x. c`.
                    let consumer = match <[String; 1]>::try_from(binders) {
                        Ok([binder]) => CoTerm::MuTilde(binder, Box::new(command)),
                        Err(binders) => CoTerm::MuTildeTensor(binders, Box::new(command)),
                    };
                    Ok(Term::Co(Box::new(consumer)))
                }
                (false, None) => Ok(Term::Co(Box::new(CoTerm::CoCase(branches)))),
                _ => Err(LowerError::Unsupported(
                    "a `select` covers either a labelled type or one product, not both".into(),
                )),
            }
        }

        Expr::Block(exprs) => {
            // A block evaluates expressions in order. A bodyless let scopes
            // over the rest of the block, so lower the rest inside its scope.
            fn lower_block(
                exprs: &[Node<Expr>],
                index: usize,
                seq_counter: &mut usize,
                continuations: &[String],
            ) -> Result<Term, LowerError> {
                let Some(e) = exprs.get(index) else {
                    return Ok(Term::Var("$unit".into()));
                };

                if let Expr::Let { name, value, body: None, .. } = &e.kind {
                    // A bodyless `let` scopes over the rest of the block, so
                    // the rest is lowered as its body. Both `let` forms use
                    // the same binding lowering.
                    let rest = lower_block(exprs, index + 1, seq_counter, continuations)?;
                    let val = lower_expr(value, continuations)?;
                    return Ok(lower_let(name, val, rest));
                }

                let rest = lower_block(exprs, index + 1, seq_counter, continuations)?;
                if index + 1 == exprs.len() {
                    return lower_expr(e, continuations);
                }
                let t = lower_expr(e, continuations)?;
                let seq_name = format!("__seq{}", *seq_counter);
                *seq_counter += 1;
                Ok(Term::Mu(
                    seq_name,
                    Box::new(Command::Cut(
                        t,
                        CoTerm::MuTilde(
                            "__discarded".into(),
                            Box::new(Command::Cut(rest, CoTerm::Covar("__tail".into()))),
                        ),
                    )),
                ))
            }

            let mut seq_counter = 0;
            lower_block(exprs, 0, &mut seq_counter, continuations)
        }
    }
}

pub fn lower_program(p: &Program) -> Result<Vec<(String, Term)>, LowerError> {
    let mut variant_labels: HashMap<String, Option<String>> = HashMap::new();
    for d in &p.decls {
        if let Decl::Enum { name, variants } = &d.kind {
            for (variant, _) in variants {
                let label = format!("{name}::{variant}");
                variant_labels.insert(label.clone(), Some(label.clone()));
                // An unqualified name is usable only while it is unambiguous.
                variant_labels
                    .entry(variant.clone())
                    .and_modify(|existing| *existing = None)
                    .or_insert(Some(label));
            }
        }
    }
    VARIANTS.with(|cell| {
        *cell.borrow_mut() = variant_labels;
    });
    let mut dtor_labels: HashMap<String, Option<String>> = HashMap::new();
    for d in &p.decls {
        if let Decl::Menu { name, items } = &d.kind {
            for (item, _) in items {
                let label = format!("{name}::{item}");
                dtor_labels.insert(label.clone(), Some(label.clone()));
                // An unqualified destructor is usable only while unambiguous.
                dtor_labels
                    .entry(item.clone())
                    .and_modify(|existing| *existing = None)
                    .or_insert(Some(label));
            }
        }
    }
    MENUS.with(|cell| {
        *cell.borrow_mut() = dtor_labels;
    });

    let mut out = Vec::new();
    for d in &p.decls {
        if let Decl::Enum { name, variants } = &d.kind {
            for (variant, _) in variants {
                // An enum value is a labelled additive injection. Variant
                // payloads are not yet constructed, so the payload is unit.
                out.push((
                    format!("{name}::{variant}"),
                    Term::Tag(format!("{name}::{variant}"), Box::new(Term::Var("$unit".into()))),
                ));
            }
        }
    }
    let constants: HashMap<String, Pattern> = p
        .decls
        .iter()
        .filter_map(|d| {
            if let Decl::Const { name, value, .. } = &d.kind {
                let pattern = match &value.kind {
                    Expr::Char(c) => Some(Pattern::Char(*c)),
                    Expr::Int(n) => Some(Pattern::Int(*n)),
                    Expr::Str(s) => Some(Pattern::Str(s.clone())),
                    Expr::Bool(b) => Some(Pattern::Bool(*b)),
                    _ => None,
                };
                pattern.map(|pattern| (name.clone(), pattern))
            } else {
                None
            }
        })
        .collect();
    CONSTANTS.with(|cell| {
        *cell.borrow_mut() = constants;
    });
    for d in &p.decls {
        match &d.kind {
            Decl::Fn { name, params, body, polarity, bounds, .. } => {
                // Multi-param fn: nest lambdas. Continuation parameters form
                // the declaration's explicit lexical continuation row.
                // Negative functions are genuine co-abstractions: each
                // continuation parameter becomes a μ binder, not a λ binder.
                let continuations: Vec<String> =
                    params.iter().filter(|p| p.is_continuation).map(|p| p.name.clone()).collect();
                let mut term = lower_expr(body, &continuations)?;
                // Binders are nested in declaration order, so a call supplies
                // arguments in the order the parameters are written. Value and
                // continuation parameters alike are λ binders — a continuation
                // is a value like any other.
                for p in params.iter().rev() {
                    term = bind_param(p, term);
                }
                // A positive function with no parameters is still called, so
                // it binds the marker a call with no arguments supplies. A
                // negative one produces a continuation and is used by name.
                if params.is_empty() && *polarity == FunctionPolarity::Positive {
                    term = Term::Lam(NO_ARGUMENTS.into(), Box::new(term));
                }
                // A bounded function takes its dictionaries outermost, before
                // the value arguments.
                term = bind_dict_params(bounds, term);
                out.push((name.clone(), term));
            }
            Decl::Command { name, value_params, continuation_params, body, bounds, .. } => {
                // mu f(x: +A) | (k: -B) { E } → λx. μk. E
                let continuations: Vec<String> =
                    continuation_params.iter().map(|p| p.name.clone()).collect();
                let mut term = lower_expr(body, &continuations)?;
                // `mu f(values) | (continuations)` is called as
                // `f(values..., continuations...)`, so the continuation
                // binders are innermost.
                for p in continuation_params.iter().rev() {
                    term = Term::Lam(p.name.clone(), Box::new(term));
                }
                for p in value_params.iter().rev() {
                    term = Term::Lam(p.name.clone(), Box::new(term));
                }
                term = bind_dict_params(bounds, term);
                out.push((name.clone(), term));
            }
            Decl::Const { name, ty: _, value } => {
                out.push((name.clone(), lower_expr(value, &[])?));
            }
            // Modules are flattened by resolution before lowering; one that
            // reaches here unresolved has nothing to lower.
            Decl::Mod { .. }
            | Decl::Use { .. }
            | Decl::Trait { .. }
            | Decl::Impl { .. }
            | Decl::Effect { .. } => {}
            Decl::Data { .. } | Decl::Enum { .. } | Decl::Menu { .. } | Decl::Form { .. } => {
                // Type declarations are handled by the checker, not lowering
            }
        }
    }
    Ok(out)
}

/// The μ binder that wraps a cut. The binder is never referenced — a command
/// has no result — but it must not capture the consumer's own name.
fn cut_binder(consumer: &Expr) -> String {
    match named_consumer(consumer) {
        Some(name) if name == CUT_BINDER => format!("{CUT_BINDER}_"),
        _ => CUT_BINDER.to_string(),
    }
}

const CUT_BINDER: &str = "__cut";

/// `let x = v; body` → `μlet. ⟨ v ∥ μ̃x. ⟨ body ∥ let ⟩ ⟩`.
///
/// A binder is `μ̃`, the value abstraction: it takes what the cut delivers
/// and runs the rest with it bound. `v · e` is application, and nothing else.
///
/// Both surface `let` forms — the expression form with an explicit body and
/// the bodyless form that scopes over the rest of its block — lower here.
fn lower_let(name: &str, value: Term, body: Term) -> Term {
    Term::Mu(
        "let".into(),
        Box::new(Command::Cut(
            value,
            CoTerm::MuTilde(
                name.to_string(),
                Box::new(Command::Cut(body, CoTerm::Covar("__tail".into()))),
            ),
        )),
    )
}

/// Wrap `body` in the λ binder a declared parameter introduces — value and
/// continuation parameters alike, since a continuation is a value.
fn bind_param(p: &Param, body: Term) -> Term {
    Term::Lam(p.name.clone(), Box::new(body))
}

/// The shape a `select` arm covers: the label it answers to, if it has one,
/// and the binders for that shape's components.
fn select_arm_shape(pattern: &Pattern) -> Result<(Option<String>, Vec<String>), LowerError> {
    fn binder(pattern: &Pattern) -> Result<String, LowerError> {
        match pattern {
            Pattern::Ident(name) => Ok(name.clone()),
            Pattern::Wildcard => Ok(UNUSED_BINDER.to_string()),
            other => Err(LowerError::Unsupported(format!(
                "a `select` arm binds each component by name; found {other:?}"
            ))),
        }
    }
    match pattern {
        // `Red`: an unqualified variant written without a payload. Any other
        // name binds the whole value: a type with no structure has one shape
        // whose single component is the value itself.
        Pattern::Ident(name) => match lookup_variant(name) {
            Some(label) => Ok((Some(label), Vec::new())),
            None => Ok((None, vec![name.clone()])),
        },
        Pattern::Wildcard => Ok((None, vec![UNUSED_BINDER.to_string()])),
        // `Color::Red(x)` or `Red(x)`.
        Pattern::Enum { name, variant, fields } => {
            let written =
                if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
            let label = lookup_variant(&written).unwrap_or(written);
            Ok((Some(label), fields.iter().map(binder).collect::<Result<_, _>>()?))
        }
        // `S { left: a, right: b }`: a struct is a labelled product.
        Pattern::Data { name, fields } => Ok((
            Some(name.clone()),
            fields.iter().map(|(_, pattern)| binder(pattern)).collect::<Result<_, _>>()?,
        )),
        // `(a, b)`: an unlabelled product.
        Pattern::Tuple(items) => Ok((None, items.iter().map(binder).collect::<Result<_, _>>()?)),
        Pattern::Dtor { .. } => Err(LowerError::Unsupported(
            "a `select` covers either a menu's requests or a data type's shapes, not both".into(),
        )),
        other => Err(LowerError::Unsupported(format!(
            "a `select` arm covers one shape of the type; found {other:?}"
        ))),
    }
}

/// The consumer a cut names, seeing through `↓`/`↑`. Both shifts erase at
/// lowering — a boxed consumer and the consumer are the same value at run
/// time — so `v @ ↑k` names `k` just as `v @ k` does.
fn named_consumer(consumer: &Expr) -> Option<&String> {
    match consumer {
        Expr::Ident(name) => Some(name),
        Expr::Shift { expr, .. } => named_consumer(&expr.kind),
        _ => None,
    }
}

/// The command a `select` arm runs. A cut against a named consumer is that
/// command directly; any other command-typed expression is lowered as a term
/// and cut against the arm's own co-variable, which nothing returns to.
fn lower_select_command(
    command: &Node<Expr>,
    continuations: &[String],
) -> Result<Command, LowerError> {
    if let Expr::Cut { value, consumer } = &command.kind
        && let Some(name) = named_consumer(&consumer.kind)
    {
        return Ok(Command::Cut(lower_expr(value, continuations)?, CoTerm::Covar(name.clone())));
    }
    Ok(Command::Cut(lower_expr(command, continuations)?, CoTerm::Covar(ARM_COVAR.into())))
}

/// The marker a call with no arguments applies its callee to. It is not unit:
/// `f()` passes nothing, while `f(())` passes the unit value.
const NO_ARGUMENTS: &str = "$no_args";

/// A binder a lowered arm introduces but never mentions.
const UNUSED_BINDER: &str = "__unused";
/// The co-variable an arm's command is cut against when it is not already a
/// cut against a named consumer. Nothing binds it: an arm does not return.
const ARM_COVAR: &str = "__arm";

fn call_curried(callee: Term, args: Vec<Term>) -> Term {
    let mut result = callee;
    for arg in args {
        result = Term::Mu(
            "__call".into(),
            Box::new(Command::Cut(
                result,
                CoTerm::App(arg, Box::new(CoTerm::Covar("__call".into()))),
            )),
        );
    }
    result
}

fn lower_boolean_operator<T>(
    op: &BinOp,
    lhs: &Node<T>,
    rhs: &Node<T>,
    span: Span,
) -> Option<Node<Expr>>
where
    T: Clone,
    Expr: From<T>,
{
    if !matches!(op, BinOp::And | BinOp::Or) {
        return None;
    }

    let rhs_body = Node { span: rhs.span, kind: rhs.kind.clone().into() };
    let (true_body, false_body) = if matches!(op, BinOp::And) {
        (rhs_body, Node { span: rhs.span, kind: Expr::Bool(false) })
    } else {
        (Node { span: rhs.span, kind: Expr::Bool(true) }, rhs_body)
    };
    let lhs_expr = lhs.kind.clone().into();
    Some(Node {
        span,
        kind: Expr::If {
            cond: Box::new(Node { span: lhs.span, kind: lhs_expr }),
            then: Box::new(true_body),
            otherwise: Some(Box::new(false_body)),
        },
    })
}

fn pattern_descriptor(pattern: &Pattern) -> String {
    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }
    fn write(pattern: &Pattern, out: &mut String) {
        match pattern {
            Pattern::Wildcard => out.push('*'),
            Pattern::Ident(name) => {
                if let Some(pattern) = lookup_constant(name) {
                    write(&pattern, out);
                } else if let Some(label) = lookup_variant(name) {
                    // A variant name is a variant pattern, not a binding.
                    write(
                        &Pattern::Enum {
                            name: label.split_once("::").map(|(e, _)| e).unwrap_or("").to_string(),
                            variant: label
                                .split_once("::")
                                .map(|(_, v)| v)
                                .unwrap_or(label.as_str())
                                .to_string(),
                            fields: Vec::new(),
                        },
                        out,
                    );
                } else {
                    out.push('$');
                    out.push_str(&escape(name));
                }
            }
            Pattern::Int(n) => {
                out.push('#');
                out.push_str(&n.to_string());
            }
            Pattern::Str(s) => {
                out.push('"');
                out.push_str(&escape(s));
                out.push('"');
            }
            Pattern::Char(c) => {
                out.push('\'');
                match c {
                    '\\' => out.push_str("\\\\"),
                    '\'' => out.push_str("\\'"),
                    c => out.push(*c),
                }
                out.push('\'');
            }
            Pattern::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
            }
            Pattern::Float(n) => {
                out.push('%');
                out.push_str(&n.to_string());
            }
            Pattern::Or(alternatives) => {
                out.push('(');
                for (i, alternative) in alternatives.iter().enumerate() {
                    if i > 0 {
                        out.push('|');
                    }
                    write(alternative, out);
                }
                out.push(')');
            }
            Pattern::Range { start, end } => {
                write(start, out);
                out.push_str("..=");
                write(end, out);
            }
            Pattern::Binding { name, pattern } => {
                out.push_str(&escape(name));
                out.push('@');
                write(pattern, out);
            }
            Pattern::Rest => out.push_str(".."),
            // A request shape: its qualified label with the continuation as
            // the single bound field, exactly as an enum pattern encodes.
            Pattern::Dtor { dtor, binder } => {
                let label = lookup_dtor(dtor).unwrap_or_else(|| dtor.clone());
                out.push('"');
                out.push_str(&escape(&label));
                out.push('"');
                out.push('(');
                out.push('$');
                out.push_str(&escape(binder));
                out.push(')');
            }
            Pattern::Tuple(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(')');
            }
            Pattern::List { items, rest } => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                if let Some(rest) = rest {
                    if !items.is_empty() {
                        out.push(',');
                    }
                    write(rest, out);
                }
                out.push(']');
            }
            Pattern::Data { name, fields } => {
                // Fields are written in declaration order, so the pattern is
                // the labelled shape the value has, with positional fields.
                out.push('"');
                out.push_str(&escape(name));
                out.push('"');
                out.push('(');
                for (i, (_, pattern)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(pattern, out);
                }
                out.push(')');
            }
            Pattern::Enum { name, variant, fields } => {
                // An unqualified constructor pattern carries the variant name
                // in `name`; resolve either spelling to the declared label.
                let written =
                    if variant.is_empty() { name.clone() } else { format!("{name}::{variant}") };
                let label = lookup_variant(&written).unwrap_or(written);
                out.push('"');
                out.push_str(&escape(&label));
                out.push('"');
                out.push('(');
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(field, out);
                }
                out.push(')');
            }
        }
    }
    let mut out = String::new();
    write(pattern, &mut out);
    out
}

fn lookup_constant(name: &str) -> Option<Pattern> {
    CONSTANTS.with(|cell| cell.borrow().get(name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn lower_str(s: &str) -> Vec<(String, Term)> {
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        lower_program(&prog).unwrap()
    }

    #[test]
    fn lower_block_sequence_names_cannot_capture_user_continuations() {
        let out = lower_str(
            "fn f(__seq0: -i32, __ret0: -i32) <- i32 {
                println(1);
                println(2);
                __seq0(1)
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(
            printed.starts_with("λ__seq0."),
            "user continuation binder should remain distinct: {printed}"
        );
        assert!(
            printed.matches("__seq0").count() >= 2,
            "user continuation must still be referenced: {printed}"
        );
    }

    #[test]
    fn lower_block_sequence_handles_nested_shadowed_empty_and_single_forms() {
        // A positive function with no parameters binds the marker a call
        // with no arguments supplies, so it stays callable.
        let single = lower_str("fn f() -> i32 { 1 }")[0].1.clone();
        assert_eq!(single, Term::Lam("$no_args".into(), Box::new(Term::Var("$int_1".into()))));

        let empty = lower_str("fn f() -> unit { }")[0].1.clone();
        assert_eq!(empty, Term::Lam("$no_args".into(), Box::new(Term::Var("$unit".into()))));

        let nested = lower_str(
            "fn f() -> i32 {
                let x = 1;
                {
                    let y = 2;
                    x
                }
            }",
        )[0]
        .1
        .clone();
        let printed = format!("{nested}");
        assert!(printed.contains("x"), "nested block lost binding: {printed}");
        assert!(printed.contains("y"), "nested block lost inner binding: {printed}");
        assert!(
            printed.matches("__seq").count() == printed.matches("__ret").count(),
            "sequence binder/co-variable names must pair: {printed}"
        );
    }

    #[test]
    fn lower_select_is_a_negative_additive_consumer() {
        // `select` must lower to a genuine negative additive co-term — one
        // branch per variant, each cutting the arm value against the arm's
        // consumer — and not to an opaque builtin marker.
        let src = "enum Color { Red, Green, Blue } fn k(return: -i32) <- Color { select Color { Red <= 0 @ return, Green <= 1 @ return, Blue <= 2 @ return } }";
        let out = lower_str(src);
        let k = out.iter().find(|(name, _)| name == "k").unwrap();

        // The declaration binds its continuation parameter as a co-abstraction.
        let Term::Lam(covar, body) = &k.1 else {
            panic!("negative function should lower to a co-abstraction: {}", k.1);
        };
        assert_eq!(covar, "return");
        let Term::Co(coterm) = body.as_ref() else {
            panic!("`select` should lower to a reified co-term: {body}");
        };
        let CoTerm::CoCase(branches) = coterm.as_ref() else {
            panic!("`select` should lower to a negative additive consumer: {coterm}");
        };
        let labels: Vec<&str> = branches.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Color::Red", "Color::Green", "Color::Blue"]);
        for (branch, value) in branches.iter().zip(["$int_0", "$int_1", "$int_2"]) {
            let Command::Cut(Term::Var(arg), CoTerm::Covar(consumer)) = branch.body.as_ref() else {
                panic!("branch should cut its value against its consumer: {}", branch.body);
            };
            assert_eq!(arg, value);
            assert_eq!(consumer, "return");
        }
    }

    #[test]
    fn lower_select_over_a_product_is_a_product_consumer() {
        // A product has one shape, so its consumer binds every component and
        // needs no label.
        let out = lower_str(
            "fn total(out: -i64) <- (+i64 ⊗ +i64) {
                 select (+i64 ⊗ +i64) { (left, right) <= (left + right) @ out }
             }",
        );
        let Term::Lam(_, body) = &out[0].1 else { panic!("expected a co-abstraction") };
        let Term::Co(coterm) = body.as_ref() else { panic!("expected a reified co-term") };
        let CoTerm::MuTildeTensor(binders, command) = coterm.as_ref() else {
            panic!("expected a product consumer: {coterm}");
        };
        assert_eq!(binders, &["left".to_string(), "right".to_string()]);
        assert!(
            matches!(command.as_ref(), Command::Cut(_, CoTerm::Covar(name)) if name == "out"),
            "{command}"
        );
    }

    #[test]
    fn lower_select_over_a_struct_binds_its_fields() {
        // A struct is a labelled product: one branch, labelled by the
        // declaration, binding every field.
        let out = lower_str(
            "data R { value: i64, unit: String }
             fn show(out: -String) <- R { select R { R { value, unit } <= unit @ out } }",
        );
        let show = out.iter().find(|(name, _)| name == "show").unwrap();
        let printed = format!("{}", show.1);
        assert!(printed.contains("μ̃[R(value, unit)."), "{printed}");
    }

    #[test]
    fn lower_enum_value_is_a_labelled_injection() {
        let out = lower_str("enum Color { Red, Green } fn main() -> i32 { 0 }");
        let red = out.iter().find(|(name, _)| name == "Color::Red").unwrap();
        assert_eq!(red.1, Term::Tag("Color::Red".into(), Box::new(Term::Var("$unit".into()))));
    }

    #[test]
    fn lower_select_rejects_an_arm_that_is_not_a_shape() {
        // An arm covers one shape of the type; a literal is not one.
        let src = "enum Color { Red, Green } fn k(return: -i32) <- Color { select Color { 1 <= 0 @ return, Green <= 1 @ return } }";
        let toks = crate::lexer::lex(src).unwrap();
        let prog = crate::parser::parse(toks).unwrap();
        assert!(
            matches!(lower_program(&prog), Err(LowerError::Unsupported(message))
                if message.contains("covers one shape")),
            "{:?}",
            lower_program(&prog)
        );
    }

    #[test]
    fn lower_struct_literal_is_a_labelled_product() {
        // A record literal is its declaration's name applied to the
        // right-nested tensor of its field values — the same labelled shape
        // an enum variant has.
        let out = lower_str(
            "data D { left: i64, right: i64 } fn f() -> i64 { use_it(D { left: 1, right: 2 }) }",
        );
        let printed = format!("{}", out.iter().find(|(name, _)| name == "f").unwrap().1);
        assert!(
            printed.contains("D(($int_1 ⊗ $int_2))"),
            "record literal should lower to a labelled product: {printed}"
        );

        // One field needs no tensor, and none is unit.
        let one = lower_str("data One { only: i64 } fn f() -> i64 { use_it(One { only: 1 }) }");
        let printed = format!("{}", one.iter().find(|(name, _)| name == "f").unwrap().1);
        assert!(printed.contains("One($int_1)"), "{printed}");
    }

    #[test]
    fn lower_cut_with_a_named_consumer_is_a_core_cut() {
        // `v @ k` is the command ⟨v ∥ k⟩. The μ binder that wraps it is never
        // referenced — a command has no result — and must not be the
        // consumer's own name, or the cut would send the value to itself.
        let positive = lower_str("fn f(k: -i32) <- i32 { 1 @ k }");
        assert_eq!(
            positive[0].1,
            Term::Lam(
                "k".into(),
                Box::new(Term::Mu(
                    "__cut".into(),
                    Box::new(Command::Cut(Term::Var("$int_1".into()), CoTerm::Covar("k".into())))
                ))
            )
        );

        // A consumer named `__cut` still receives the value.
        let shadowed = lower_str("fn f(__cut: -i32) <- i32 { 1 @ __cut }");
        assert_eq!(
            shadowed[0].1,
            Term::Lam(
                "__cut".into(),
                Box::new(Term::Mu(
                    "__cut_".into(),
                    Box::new(Command::Cut(
                        Term::Var("$int_1".into()),
                        CoTerm::Covar("__cut".into())
                    ))
                ))
            )
        );
    }

    #[test]
    fn lower_cut_with_a_computed_consumer_evaluates_then_cuts() {
        // A consumer that is not a name is an expression producing one:
        // evaluate it, then apply it to the value — an application stack.
        let out = lower_str("fn f(ignored: +i32) -> i32 { 1 @ pick(2) }");
        let Term::Lam(_, body) = &out[0].1 else { panic!("expected a value binder") };
        let Term::Mu(binder, command) = body.as_ref() else {
            panic!("a cut is wrapped in a μ binder: {body}");
        };
        assert_eq!(binder, "__cut");
        let Command::Cut(consumer, CoTerm::App(value, _)) = command.as_ref() else {
            panic!("a computed consumer is applied to the value: {command}");
        };
        assert!(
            format!("{consumer}").contains("pick"),
            "the consumer expression is evaluated: {consumer}"
        );
        assert_eq!(value, &Term::Var("$int_1".into()));
    }

    #[test]
    fn lower_int() {
        let out = lower_str("42");
        assert_eq!(out[0].1, Term::Lam("$no_args".into(), Box::new(Term::Var("$int_42".into()))));
    }

    #[test]
    fn lower_negative_fn_binds_continuation_parameters_as_lambdas() {
        // A continuation is a value like any other, so a negative function's
        // continuation parameters are ordinary λ binders, nested in
        // declaration order.
        let out = lower_str("fn k(return: -i32, other: -bool) <- bool { return(0) }");
        let term = &out[0].1;
        let Term::Lam(first, rest) = term else {
            panic!("continuation parameter must be a λ binder: {term}");
        };
        assert_eq!(first, "return");
        assert!(
            matches!(rest.as_ref(), Term::Lam(second, _) if second == "other"),
            "second continuation parameter must also be a λ binder: {rest}"
        );
    }

    #[test]
    fn lower_mu_binds_values_before_continuations() {
        // `mu f(values) | (continuations)` is called as
        // `f(values..., continuations...)`, so the λ binders come first.
        let out = lower_str("command route(x: +i32) | (k: -i32) { k(x) }");
        let term = &out[0].1;
        let Term::Lam(value, rest) = term else {
            panic!("value parameter must be a λ binder: {term}");
        };
        assert_eq!(value, "x");
        assert!(
            matches!(rest.as_ref(), Term::Lam(k, _) if k == "k"),
            "continuation parameter must be a co-abstraction binder: {rest}"
        );
    }

    #[test]
    fn lower_lambda() {
        let out = lower_str("fn id(x: +i32) -> i32 { x }");
        assert_eq!(out[0].1, Term::Lam("x".into(), Box::new(Term::Var("x".into()))));
    }

    #[test]
    fn lower_boolean_operators_expand_to_lazy_if() {
        let out = lower_str("true && false");
        assert!(
            !format!("{}", out[0].1).contains("__unimplemented_boolean_operator"),
            "boolean `and` must not lower to an unimplemented marker: {}",
            out[0].1
        );

        let out = lower_str("false || true");
        assert!(
            !format!("{}", out[0].1).contains("__unimplemented_boolean_operator"),
            "boolean `or` must not lower to an unimplemented marker: {}",
            out[0].1
        );
    }

    #[test]
    fn lower_negative_fn() {
        let out = lower_str("fn k(x: -i32) <- i32 { x }");
        assert_eq!(out[0].1, Term::Lam("x".into(), Box::new(Term::Var("x".into()))));
    }

    #[test]
    fn lower_uses_explicit_lexical_continuation_scopes() {
        // A local mu adds its binder only inside its own body.
        let out = lower_str(
            "fn f(ok: -i32) <- i32 {
                mu escape(inner: -i32) { ok(escape(1, inner)) }
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("inner"), "local mu binder missing: {printed}");
    }

    #[test]
    fn generic_positive_and_negative_functions_lower_structurally() {
        let positive = lower_str("fn id<T>(value: T) -> T { value }")[0].1.clone();
        assert!(
            matches!(&positive, Term::Lam(name, body) if name == "value" && matches!(&**body, Term::Var(v) if v == "value")),
            "positive generic parameter should lower as a lambda binder: {positive:?}"
        );

        let negative = lower_str("fn k<T>(ok: -T) <- T { ok(0) }")[0].1.clone();
        assert!(
            matches!(&negative, Term::Lam(name, _) if name == "ok"),
            "negative generic continuation parameter should lower as a co-abstraction binder: {negative:?}"
        );
        let printed = format!("{negative}");
        assert!(printed.starts_with("λok."), "continuation binder missing: {printed}");
    }

    #[test]
    fn lower_types() {
        let ty = lower_type(&TypeExpr::Base("i32".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::I32));
        let ty = lower_type(&TypeExpr::Base("bool".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::Bool));
    }

    #[test]
    fn lower_type_level_dual_remains_supported() {
        let inner = Node {
            span: crate::token::Span { start: 5, end: 8 },
            kind: TypeExpr::Base("i32".into()),
        };
        // `dual(A)` applies the involution rather than wrapping a node.
        let ty = lower_type(&TypeExpr::Dual(Box::new(inner))).unwrap();
        assert_eq!(ty, Type::Neg(Base::I32));
    }

    #[test]
    fn lower_negative_type() {
        // -i32 → Neg(I32)
        let inner = Node {
            span: crate::token::Span { start: 0, end: 4 },
            kind: TypeExpr::Base("i32".into()),
        };
        let ty = lower_type(&TypeExpr::Negative(Box::new(inner))).unwrap();
        assert_eq!(ty, Type::Neg(Base::I32));
    }
}
