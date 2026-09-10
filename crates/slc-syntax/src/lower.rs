//! Lowering from surface AST to λ̄μμ̃ core IR.

use crate::ast::*;
use crate::token::Span;
use slc_core::command::Command;
use slc_core::coterm::{CoCaseBranch, CoTerm};
use slc_core::term::Term;
use slc_core::types::{Base, Type};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CONSTANTS: RefCell<HashMap<String, Pattern>> = RefCell::new(HashMap::new());
    /// Variant name → fully qualified label, for every declared enum. An
    /// unqualified variant name is recorded only when it is unambiguous.
    static VARIANTS: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
}

/// The fully qualified label a variant path or unambiguous variant name
/// denotes, if it names a declared enum variant.
fn lookup_variant(name: &str) -> Option<String> {
    VARIANTS.with(|cell| cell.borrow().get(name).cloned().flatten())
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
            let mut result = lower_expr(callee, continuations)?;
            let args = if args.is_empty() {
                // A call with no arguments still applies its callee, to the
                // marker that carries none.
                vec![Term::Var(NO_ARGUMENTS.into())]
            } else {
                args.iter().map(|e| lower_expr(e, continuations)).collect::<Result<Vec<_>, _>>()?
            };
            for arg in args {
                result = Term::Mu(
                    "__call".into(),
                    Box::new(Command::Cut(
                        result,
                        CoTerm::CoLam(
                            "__f".into(),
                            Box::new(Command::Cut(arg, CoTerm::Covar("__call".into()))),
                        ),
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
            // Build: μif. ⟨ __if_dispatch(cond, λt, λe) ∥ λ̄__f. ⟨ __f ∥ if ⟩ ⟩
            // The dispatch builtin applies the chosen thunk to unit.
            let triple = Term::Pair(
                Box::new(Term::Var("__cond".into())),
                Box::new(Term::Pair(Box::new(t), Box::new(e))),
            );
            let dispatch_call = Term::Mu(
                "__call".into(),
                Box::new(Command::Cut(
                    Term::Var("__if_dispatch".into()),
                    CoTerm::CoLam(
                        "__f".into(),
                        Box::new(Command::Cut(triple, CoTerm::Covar("__call".into()))),
                    ),
                )),
            );
            Ok(Term::Mu(
                "__if".into(),
                Box::new(Command::Cut(
                    c,
                    CoTerm::MuTilde(
                        "__cond".into(),
                        Box::new(Command::Cut(dispatch_call, CoTerm::Covar("__if".into()))),
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
            let command = match &consumer.kind {
                // A named consumer is a co-variable, so the cut is direct.
                Expr::Ident(name) => Command::Cut(v, CoTerm::Covar(name.clone())),
                // Any other consumer is an expression that produces one, so
                // the cut activates the continuation it evaluates to.
                _ => Command::Activate(lower_expr(consumer, continuations)?, v),
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
        Expr::ErrorProp { expr, continuation } => {
            let selected = continuation.clone().or_else(|| continuations.last().cloned());
            if let Some(name) = selected {
                // e?err applies e to the named error continuation.
                let e = lower_expr(expr, continuations)?;
                return Ok(call_curried(e, vec![Term::Var(name.clone())]));
            }
            // e? → μprop. ⟨ e' ∥ λ̄__ok. ⟨ __ok ∥ prop ⟩ ⟩
            // The value flows to the success continuation; errors escape
            // via the mu binder (the error continuation).
            let e = lower_expr(expr, continuations)?;
            Ok(Term::Mu(
                "__err".into(),
                Box::new(Command::Cut(
                    e,
                    CoTerm::MuTilde(
                        "__ok".into(),
                        Box::new(Command::Cut(
                            Term::Var("__ok".into()),
                            CoTerm::Covar("__err".into()),
                        )),
                    ),
                )),
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
                arm_terms.push(Term::Inl(Box::new(Term::Pair(
                    Box::new(descriptor),
                    Box::new(Term::Pair(
                        Box::new(guard),
                        Box::new(Term::Lam("__match_arg".into(), Box::new(b))),
                    )),
                ))));
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
                    CoTerm::CoLam(
                        "__f".into(),
                        Box::new(Command::Cut(payload, CoTerm::Covar("__match".into()))),
                    ),
                )),
            ))
        }

        Expr::Struct { name, fields } => {
            // A struct value is a labelled product: the declaration's name
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

        Expr::Select { arms, .. } => {
            // `select T { p <= c, … }` is the consumer of T, given by cases
            // on it: one branch per shape, binding that shape's components.
            //
            //   labelled (enum, struct) ⟹ co(μ̃[ L(x…). c | … ])
            //   product (tensor)        ⟹ co(μ̃(x…). c)
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
                let covar = format!("__ret{}", *seq_counter);
                *seq_counter += 1;
                Ok(Term::Mu(
                    seq_name,
                    Box::new(Command::Cut(
                        t,
                        CoTerm::MuTilde(
                            "__discarded".into(),
                            Box::new(Command::Cut(rest, CoTerm::Covar(covar))),
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
            Decl::Fn { name, params, body, polarity, .. } => {
                // Multi-param fn: nest lambdas. Continuation parameters form
                // the declaration's explicit lexical continuation row.
                // Negative functions are genuine co-abstractions: each
                // continuation parameter becomes a μ binder, not a λ binder.
                let continuations: Vec<String> =
                    params.iter().filter(|p| p.is_continuation).map(|p| p.name.clone()).collect();
                let mut term = lower_expr(body, &continuations)?;
                // Binders are nested in declaration order, so a call supplies
                // arguments in the order the parameters are written. A value
                // parameter is a λ binder; a continuation parameter is a Λ
                // co-abstraction binder, never an ordinary λ.
                for p in params.iter().rev() {
                    term = bind_param(p, term);
                }
                // A positive function with no parameters is still called, so
                // it binds the marker a call with no arguments supplies. A
                // negative one produces a continuation and is used by name.
                if params.is_empty() && *polarity == FunctionPolarity::Positive {
                    term = Term::Lam(NO_ARGUMENTS.into(), Box::new(term));
                }
                out.push((name.clone(), term));
            }
            Decl::Command { name, value_params, continuation_params, body, .. } => {
                // mu f(x: +A) | (k: -B) { E } → λx. μk. E
                let continuations: Vec<String> =
                    continuation_params.iter().map(|p| p.name.clone()).collect();
                let mut term = lower_expr(body, &continuations)?;
                // `mu f(values) | (continuations)` is called as
                // `f(values..., continuations...)`, so the continuation
                // binders are innermost.
                for p in continuation_params.iter().rev() {
                    term = Term::CoAbs(p.name.clone(), Box::new(term));
                }
                for p in value_params.iter().rev() {
                    term = Term::Lam(p.name.clone(), Box::new(term));
                }
                out.push((name.clone(), term));
            }
            Decl::Const { name, ty: _, value } => {
                out.push((name.clone(), lower_expr(value, &[])?));
            }
            Decl::Struct { .. } | Decl::Enum { .. } => {
                // Type declarations are handled by the checker, not lowering
            }
        }
    }
    Ok(out)
}

/// The μ binder that wraps a cut. The binder is never referenced — a command
/// has no result — but it must not capture the consumer's own name.
fn cut_binder(consumer: &Expr) -> String {
    match consumer {
        Expr::Ident(name) if name == CUT_BINDER => format!("{CUT_BINDER}_"),
        _ => CUT_BINDER.to_string(),
    }
}

const CUT_BINDER: &str = "__cut";

/// `let x = v; body` → `μlet. ⟨ v ∥ μ̃x. ⟨ body ∥ let ⟩ ⟩`.
///
/// A binder is `μ̃`, the value abstraction: it takes what the cut delivers
/// and runs the rest with it bound. `λ̄` is application, and nothing else.
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
                Box::new(Command::Cut(body, CoTerm::Covar("let".into()))),
            ),
        )),
    )
}

/// Wrap `body` in the binder a declared parameter introduces: a λ binder for
/// a value parameter, a Λ co-abstraction binder for a continuation parameter.
fn bind_param(p: &Param, body: Term) -> Term {
    if p.is_continuation {
        Term::CoAbs(p.name.clone(), Box::new(body))
    } else {
        Term::Lam(p.name.clone(), Box::new(body))
    }
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
        Pattern::Struct { name, fields } => Ok((
            Some(name.clone()),
            fields.iter().map(|(_, pattern)| binder(pattern)).collect::<Result<_, _>>()?,
        )),
        // `(a, b)`: an unlabelled product.
        Pattern::Tuple(items) => Ok((None, items.iter().map(binder).collect::<Result<_, _>>()?)),
        other => Err(LowerError::Unsupported(format!(
            "a `select` arm covers one shape of the type; found {other:?}"
        ))),
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
        && let Expr::Ident(name) = &consumer.kind
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
                CoTerm::CoLam(
                    "__f".into(),
                    Box::new(Command::Cut(arg, CoTerm::Covar("__call".into()))),
                ),
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
            Pattern::Struct { name, fields } => {
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
            printed.starts_with("Λ__seq0."),
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
        let Term::CoAbs(covar, body) = &k.1 else {
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
        let Term::CoAbs(_, body) = &out[0].1 else { panic!("expected a co-abstraction") };
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
            "struct R { value: i64, unit: String }
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
        // A struct literal is its declaration's name applied to the
        // right-nested tensor of its field values — the same labelled shape
        // an enum variant has.
        let out = lower_str(
            "struct D { left: i64, right: i64 } fn f() -> i64 { use_it(D { left: 1, right: 2 }) }",
        );
        let printed = format!("{}", out.iter().find(|(name, _)| name == "f").unwrap().1);
        assert!(
            printed.contains("D(($int_1 ⊗ $int_2))"),
            "struct literal should lower to a labelled product: {printed}"
        );

        // One field needs no tensor, and none is unit.
        let one = lower_str("struct One { only: i64 } fn f() -> i64 { use_it(One { only: 1 }) }");
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
            Term::CoAbs(
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
            Term::CoAbs(
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
    fn lower_cut_with_a_computed_consumer_activates_it() {
        // A consumer that is not a name is an expression producing one, so
        // the cut activates the continuation it evaluates to.
        let out = lower_str("fn f(ignored: +i32) -> i32 { 1 @ pick(2) }");
        let Term::Lam(_, body) = &out[0].1 else { panic!("expected a value binder") };
        let Term::Mu(binder, command) = body.as_ref() else {
            panic!("a cut is wrapped in a μ binder: {body}");
        };
        assert_eq!(binder, "__cut");
        let Command::Activate(consumer, value) = command.as_ref() else {
            panic!("a computed consumer is activated: {command}");
        };
        assert_eq!(value, &Term::Var("$int_1".into()));
        assert!(
            format!("{consumer}").contains("pick"),
            "the consumer expression is evaluated: {consumer}"
        );
    }

    #[test]
    fn lower_int() {
        let out = lower_str("42");
        assert_eq!(out[0].1, Term::Lam("$no_args".into(), Box::new(Term::Var("$int_42".into()))));
    }

    #[test]
    fn lower_negative_fn_uses_co_abstraction_binders_for_continuation_parameters() {
        // Every parameter of a negative function is a continuation, so every
        // binder is a co-abstraction, nested in declaration order.
        let out = lower_str("fn k(return: -i32, other: -bool) <- bool { return(0) }");
        let term = &out[0].1;
        let Term::CoAbs(first, rest) = term else {
            panic!("continuation parameter must be a co-abstraction binder: {term}");
        };
        assert_eq!(first, "return");
        assert!(
            matches!(rest.as_ref(), Term::CoAbs(second, _) if second == "other"),
            "second continuation parameter must also be a co-abstraction: {rest}"
        );
        let printed = format!("{term}");
        assert!(
            !printed.contains("λreturn.") && !printed.contains("λother."),
            "a continuation parameter must not lower to a λ binder: {printed}"
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
            matches!(rest.as_ref(), Term::CoAbs(k, _) if k == "k"),
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
        assert_eq!(out[0].1, Term::CoAbs("x".into(), Box::new(Term::Var("x".into()))));
    }

    /// The argument of a lowered application `f(a)`.
    fn applied_argument(term: &Term) -> Option<&Term> {
        let Term::Mu(call, command) = term else { return None };
        if call != "__call" {
            return None;
        }
        let Command::Cut(_, CoTerm::CoLam(marker, inner)) = command.as_ref() else {
            return None;
        };
        if marker != "__f" {
            return None;
        }
        let Command::Cut(arg, CoTerm::Covar(_)) = inner.as_ref() else { return None };
        Some(arg)
    }

    #[test]
    fn lower_named_error_prop_applies_the_named_continuation() {
        // `e?err` supplies `err` to `e` as its error continuation.
        let out = lower_str("command f(x: +i32) | (ok: -i32, err: -i32) { fail(x)?err }");
        let Term::Lam(_, body) = &out[0].1 else { panic!("expected a value binder") };
        let Term::CoAbs(_, body) = body.as_ref() else { panic!("expected `ok` binder") };
        let Term::CoAbs(_, body) = body.as_ref() else { panic!("expected `err` binder") };
        assert_eq!(applied_argument(body), Some(&Term::Var("err".into())));
    }

    #[test]
    fn lower_bare_error_prop_applies_the_innermost_continuation() {
        // A bare `?` supplies the current error continuation: the last
        // continuation declared by the innermost enclosing row.
        let out = lower_str("command f(x: +i32) | (ok: -i32, err: -i32) { fail(x)? }");
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("err"), "bare `?` should select `err`: {printed}");

        // A nested local `mu` extends the row, so its binder wins inside it.
        let out = lower_str(
            "command f(x: +i32) | (err: -i32) {
                mu inner(nested: -i32) { fail(x)? }
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(
            printed.contains("⟨nested ∥ __call⟩"),
            "nested bare `?` should select the innermost binder: {printed}"
        );
    }

    #[test]
    fn lower_error_prop_uses_current_continuation() {
        let out = lower_str("command f(x: +i32) | (err: -i32) { fail(x)? }");
        let term = &out[0].1;
        let printed = format!("{term}");
        assert!(printed.contains("err"), "should reference err: {printed}");
    }

    #[test]
    fn lower_uses_explicit_lexical_continuation_scopes() {
        // A lambda must not inherit the surrounding declaration's
        // continuation row: bare `?` cannot accidentally select `err`.
        let out =
            lower_str("command f(x: +i32) | (err: -i32) { g(fn(y: +i32) -> i32 { fail(y)? }) }");
        let printed = format!("{}", out[0].1);
        assert!(!printed.contains("err(y)"), "lambda leaked `err`: {printed}");

        // A local mu adds its binder only inside its own body.
        let out = lower_str(
            "fn f(ok: -i32) <- i32 {
                mu escape(inner: -i32) { ok(escape(1, inner)) }
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("inner"), "local mu binder missing: {printed}");

        // Named selected propagation resolves through nested mu scopes.
        let out = lower_str(
            "command outer(x: +i32) | (err: -i32) {
                mu inner(inner_err: -i32) { fail(x)?err }
            }",
        );
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("err"), "outer continuation missing: {printed}");
    }

    #[test]
    fn lower_named_error_prop_selects_continuation() {
        let out = lower_str("fn f() -> i32 { fail(x)?missing }");
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("missing"), "should reference missing: {printed}");
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
            matches!(&negative, Term::CoAbs(name, _) if name == "ok"),
            "negative generic continuation parameter should lower as a co-abstraction binder: {negative:?}"
        );
        let printed = format!("{negative}");
        assert!(printed.starts_with("Λok."), "co-abstraction binder missing: {printed}");
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
