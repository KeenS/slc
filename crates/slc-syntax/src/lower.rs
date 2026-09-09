//! Lowering from surface AST to λ̄μμ̃ core IR.

use crate::ast::*;
use crate::token::Span;
use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use slc_core::types::{Base, Type};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CONSTANTS: RefCell<HashMap<String, Pattern>> = RefCell::new(HashMap::new());
    static CONTINUATIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn current_continuation() -> Option<String> {
    CONTINUATIONS.with(|cell| cell.borrow().last().cloned())
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
        TypeExpr::Positive(inner) => Ok(lower_type(&inner.kind)?.dual().dual()),
        TypeExpr::Negative(inner) => Ok(lower_type(&inner.kind)?.dual()),
        TypeExpr::Tensor(a, b) => {
            Ok(Type::Tensor(Box::new(lower_type(&a.kind)?), Box::new(lower_type(&b.kind)?)))
        }
        TypeExpr::Par(a, b) => {
            Ok(Type::Par(Box::new(lower_type(&a.kind)?), Box::new(lower_type(&b.kind)?)))
        }
        TypeExpr::Fun(a, b) => {
            Ok(Type::Fun(Box::new(lower_type(&a.kind)?), Box::new(lower_type(&b.kind)?)))
        }
        TypeExpr::List(inner) => Ok(Type::List(Box::new(lower_type(&inner.kind)?))),
        TypeExpr::Dual(inner) => Ok(Type::Dual(Box::new(lower_type(&inner.kind)?))),
        TypeExpr::Command(input, output) => {
            // Command<I, O> ≡ dual(I) ⅋ O.
            let i = lower_type(&input.kind)?;
            let o = lower_type(&output.kind)?;
            Ok(Type::Par(Box::new(i.dual()), Box::new(o)))
        }
        TypeExpr::Unit => Ok(Type::One),
        TypeExpr::Bottom => Ok(Type::Bottom),
    }
}

/// Lower an expression to a core term.
pub fn lower_expr(e: &Node<Expr>) -> Result<Term, LowerError> {
    match &e.kind {
        Expr::Int(n) => Ok(Term::Var(format!("$int_{n}"))),
        Expr::Float(n) => Ok(Term::Var(format!("$float_{n}"))),
        Expr::Str(s) => Ok(Term::Var(format!("$str_{s:?}"))),
        Expr::Char(c) => Ok(Term::Var(format!("$char_{c}"))),
        Expr::Bool(b) => Ok(Term::Var(if *b { "true" } else { "false" }.to_string())),
        Expr::Ident(s) => {
            // Enum constructors evaluate to their name as a string.
            // The driver injects constructor globals; the fallback here
            // keeps ordinary identifiers as variables.
            Ok(Term::Var(s.clone()))
        }

        Expr::Lambda { param, param_type: _, return_type: _, body } => {
            // fn(x) { body } → λx. body'
            let saved = current_continuation();
            CONTINUATIONS.with(|cell| cell.borrow_mut().clear());
            let b = lower_expr(body)?;
            CONTINUATIONS.with(|cell| {
                if let Some(name) = saved {
                    cell.borrow_mut().push(name);
                }
            });
            Ok(Term::Lam(param.clone(), Box::new(b)))
        }

        Expr::Mu { binder, return_type: _, body } => {
            // mu(k: -T) { body } → the body, with the binder noted.
            // k(v) inside the body means "escape with v". The evaluator
            // treats k as the mu's continuation via Activate.
            let b = lower_expr(body)?;
            let a = binder.as_ref().map(|(n, _)| n.clone()).unwrap_or_else(|| "k".into());
            Ok(Term::Mu(a, Box::new(Command::Cut(b, CoTerm::Covar("__answer".into())))))
        }

        Expr::Call { callee, args } => {
            // f(a, b) lowers to nested single-argument applications:
            //   f(a) applied to (b)
            // Multi-arg functions are curried: fn f(x, y) → λx. λy. body.
            let mut result = lower_expr(callee)?;
            let args = if args.is_empty() {
                vec![Term::Var("$unit".into())]
            } else {
                args.iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?
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
                terms.push(lower_expr(item)?);
            }
            match terms.len() {
                0 => Ok(Term::Var("unit".into())),
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
            // let x = v; body → μlet. ⟨ v ∥ λ̄x. ⟨ body' ∥ let ⟩ ⟩
            let v = lower_expr(value)?;
            let b = body
                .as_ref()
                .map(|b| lower_expr(b))
                .transpose()?
                .unwrap_or_else(|| Term::Var("$unit".into()));
            Ok(Term::Mu(
                "let".into(),
                Box::new(Command::Cut(
                    v,
                    CoTerm::CoLam(
                        name.clone(),
                        Box::new(Command::Cut(b, CoTerm::Covar("let".into()))),
                    ),
                )),
            ))
        }

        Expr::If { cond, then, otherwise } => {
            // Branches are wrapped in λ so they are only evaluated when
            // chosen — if must be lazy, or mu escapes in the untaken branch
            // would fire eagerly.
            let c = lower_expr(cond)?;
            let t = Term::Lam("__unused".into(), Box::new(lower_expr(then)?));
            let e = otherwise
                .as_ref()
                .map(|e| lower_expr(e).map(|b| Term::Lam("__unused".into(), Box::new(b))))
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
                    CoTerm::CoLam(
                        "__cond".into(),
                        Box::new(Command::Cut(dispatch_call, CoTerm::Covar("__if".into()))),
                    ),
                )),
            ))
        }

        Expr::BinOp { op, lhs, rhs } => {
            if matches!(op, BinOp::And | BinOp::Or) {
                let rhs_body = (**rhs).clone();
                let rhs_span = rhs.span;
                let true_body = if matches!(op, BinOp::And) {
                    rhs_body.clone()
                } else {
                    Node { span: rhs_span, kind: Expr::Bool(true) }
                };
                let false_body = if matches!(op, BinOp::And) {
                    Node { span: rhs_span, kind: Expr::Bool(false) }
                } else {
                    rhs_body
                };
                let expanded = Node {
                    span: Span { start: lhs.span.start, end: rhs_span.end },
                    kind: Expr::If {
                        cond: Box::new((**lhs).clone()),
                        then: Box::new(true_body),
                        otherwise: Some(Box::new(false_body)),
                    },
                };
                return lower_expr(&expanded);
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
                BinOp::And | BinOp::Or => {
                    return Ok(Term::Var("__unimplemented_boolean_operator".into()));
                }
            };
            Ok(call_curried(Term::Var(name.into()), vec![lower_expr(lhs)?, lower_expr(rhs)?]))
        }

        Expr::UnOp { op, body } => {
            if matches!(op, UnOp::Not) {
                let arg = lower_expr(body)?;
                return Ok(call_curried(
                    Term::Var("eq".into()),
                    vec![arg, Term::Var("false".into())],
                ));
            }
            Ok(call_curried(Term::Var("neg".into()), vec![lower_expr(body)?]))
        }

        Expr::Index { value, index } => Ok(call_curried(
            Term::Var("__index".into()),
            vec![lower_expr(value)?, lower_expr(index)?],
        )),

        Expr::Slice { value, start, end } => {
            let start_term = start
                .as_ref()
                .map(|e| lower_expr(e))
                .transpose()?
                .unwrap_or_else(|| Term::Var("$int_0".into()));
            let end_term = end
                .as_ref()
                .map(|e| lower_expr(e))
                .transpose()?
                .unwrap_or_else(|| Term::Var("__string_len".into()));
            Ok(call_curried(
                Term::Var("substring".into()),
                vec![lower_expr(value)?, start_term, end_term],
            ))
        }

        Expr::Interaction { left, right } => {
            // `t @ e` is the surface spelling of a cut. The producer is
            // lowered normally. If the consumer is a name (as in `42 @ k`),
            // it names the continuation to which the cut sends the value.
            // For a compound consumer, lower it and apply it as a closure.
            let t = lower_expr(left)?;
            match &right.kind {
                Expr::Ident(name) => Ok(Term::Mu(
                    name.clone(),
                    Box::new(Command::Cut(t, CoTerm::Covar(name.clone()))),
                )),
                _ => {
                    let consumer = lower_expr(right)?;
                    Ok(Term::Mu(
                        "__interaction".into(),
                        Box::new(Command::Cut(
                            consumer,
                            CoTerm::CoLam(
                                "__f".into(),
                                Box::new(Command::Cut(t, CoTerm::Covar("__interaction".into()))),
                            ),
                        )),
                    ))
                }
            }
        }
        Expr::Spawn { body: _ } => Err(LowerError::Unsupported("spawn".into())),
        Expr::Dual { body } => {
            // `dual(e)` is an explicit polarity flip. At the term level it
            // denotes the same witness as `e`; the type checker is
            // responsible for viewing it with the opposite polarity.
            lower_expr(body)
        }
        Expr::ErrorProp { expr, continuation } => {
            let selected = continuation.clone().or_else(current_continuation);
            if let Some(name) = selected {
                // e?err applies e to the named error continuation.
                let e = lower_expr(expr)?;
                return Ok(call_curried(e, vec![Term::Var(name.clone())]));
            }
            // e? → μprop. ⟨ e' ∥ λ̄__ok. ⟨ __ok ∥ prop ⟩ ⟩
            // The value flows to the success continuation; errors escape
            // via the mu binder (the error continuation).
            let e = lower_expr(expr)?;
            Ok(Term::Mu(
                "__err".into(),
                Box::new(Command::Cut(
                    e,
                    CoTerm::CoLam(
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
            let s = lower_expr(scrutinee)?;
            // Encode arms as thunks: one closure per arm.
            // Each arm is a Lam so it is only evaluated when selected.
            let mut arm_terms = Vec::new();
            for arm in arms {
                let descriptor = Term::Var(format!("$str_{}", pattern_descriptor(&arm.pattern)));
                let b = lower_expr(&arm.body)?;
                let guard = match &arm.guard {
                    Some(guard) => lower_expr(guard)?,
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

        Expr::Service { agent, continuations } => {
            let mut args = vec![lower_expr(agent)?];
            for k in continuations {
                args.push(lower_expr(k)?);
            }
            Ok(call_curried(Term::Var("__service".into()), args))
        }
        Expr::Job { agent, values } => {
            let mut args = vec![lower_expr(agent)?];
            for v in values {
                args.push(lower_expr(v)?);
            }
            Ok(call_curried(Term::Var("__job".into()), args))
        }

        Expr::Block(exprs) => {
            // A block evaluates expressions in order. A trailing `let`
            // scopes over the rest of the block, so fold from the end:
            // `let x = v; rest` becomes `let x = v in rest`.
            let mut acc: Option<Term> = None;
            let mut seq_counter = 0;
            for e in exprs.iter().rev() {
                acc = Some(match (&e.kind, acc) {
                    // Bodyless let followed by the rest: scope the rest
                    (Expr::Let { name, value, body: None, .. }, Some(rest)) => {
                        let val = lower_expr(value)?;
                        Term::Mu(
                            "__let".into(),
                            Box::new(Command::Cut(
                                val,
                                CoTerm::CoLam(
                                    name.clone(),
                                    Box::new(Command::Cut(
                                        rest.clone(),
                                        CoTerm::Covar("__let".into()),
                                    )),
                                ),
                            )),
                        )
                    }
                    // Normal expression: evaluate, discard, continue
                    (_, Some(rest)) => {
                        let t = lower_expr(e)?;
                        let seq_name = format!("__seq{seq_counter}");
                        let covar = format!("__ret{seq_counter}");
                        seq_counter += 1;
                        Term::Mu(
                            seq_name,
                            Box::new(Command::Cut(
                                t,
                                CoTerm::CoLam(
                                    "__discarded".into(),
                                    Box::new(Command::Cut(rest, CoTerm::Covar(covar))),
                                ),
                            )),
                        )
                    }
                    // Final element: lower directly
                    (_, None) => lower_expr(e)?,
                });
            }
            Ok(acc.unwrap_or_else(|| Term::Var("$unit".into())))
        }

        Expr::CommandDef { name: _, params: _, body: _ } => {
            Err(LowerError::Unsupported("command expressions".into()))
        }
    }
}

pub fn lower_program(p: &Program) -> Result<Vec<(String, Term)>, LowerError> {
    let mut out = Vec::new();
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
            Decl::Fn { name, params, return_type: _, body, type_params: _ } => {
                // Multi-param fn: nest lambdas
                let continuations: Vec<String> =
                    params.iter().filter(|p| p.is_continuation).map(|p| p.name.clone()).collect();
                CONTINUATIONS.with(|cell| {
                    cell.borrow_mut().extend(continuations.iter().cloned());
                });
                let mut term = lower_expr(body)?;
                CONTINUATIONS.with(|cell| {
                    for _ in &continuations {
                        cell.borrow_mut().pop();
                    }
                });
                for p in params.iter().rev() {
                    term = Term::Lam(p.name.clone(), Box::new(term));
                }
                out.push((name.clone(), term));
            }
            Decl::Command { name, params, body } => {
                // command f(x, to k) { E } → κx. μα. E
                let continuations: Vec<String> =
                    params.iter().filter(|p| p.is_continuation).map(|p| p.name.clone()).collect();
                CONTINUATIONS.with(|cell| {
                    cell.borrow_mut().extend(continuations.iter().cloned());
                });
                let mut term = lower_expr(body)?;
                CONTINUATIONS.with(|cell| {
                    for _ in &continuations {
                        cell.borrow_mut().pop();
                    }
                });
                for p in params.iter().rev() {
                    if p.is_continuation {
                        term = Term::Mu(
                            p.name.clone(),
                            Box::new(Command::Cut(term, CoTerm::Covar(p.name.clone()))),
                        );
                    } else {
                        term = Term::Lam(p.name.clone(), Box::new(term));
                    }
                }
                out.push((name.clone(), term));
            }
            Decl::Const { name, ty: _, value } => {
                out.push((name.clone(), lower_expr(value)?));
            }
            Decl::Struct { .. } | Decl::Enum { .. } => {
                // Type declarations are handled by the checker, not lowering
            }
        }
    }
    Ok(out)
}

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
                out.push_str(&escape(name));
                out.push('{');
                for (i, (field, pattern)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&escape(field));
                    out.push(':');
                    write(pattern, out);
                }
                out.push('}');
            }
            Pattern::Enum { name, variant, fields } => {
                out.push('"');
                out.push_str(&escape(name));
                out.push_str("::");
                out.push_str(&escape(variant));
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
    fn lower_int() {
        let out = lower_str("42");
        assert_eq!(out[0].1, Term::Var("$int_42".into()));
    }

    #[test]
    fn lower_lambda() {
        let out = lower_str("fn id(x: +i32) -> i32 { x }");
        assert_eq!(out[0].1, Term::Lam("x".into(), Box::new(Term::Var("x".into()))));
    }

    #[test]
    fn lower_mu() {
        let out = lower_str("mu(k: -i32) { k(42) }");
        // body is a call k(42), which lowers to just Var(k) currently
        assert!(matches!(out[0].1, Term::Mu(_, _)));
    }

    #[test]
    fn lower_error_prop_uses_current_continuation() {
        let out = lower_str("command f(x: +i32, to err: -i32) { fail(x)? }");
        let term = &out[0].1;
        let printed = format!("{term}");
        assert!(printed.contains("err"), "should reference err: {printed}");
    }

    #[test]
    fn lower_named_error_prop_selects_continuation() {
        let out = lower_str("fn f() -> i32 { fail(x)?missing }");
        let printed = format!("{}", out[0].1);
        assert!(printed.contains("missing"), "should reference missing: {printed}");
    }

    #[test]
    fn lower_types() {
        let ty = lower_type(&TypeExpr::Base("i32".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::I32));
        let ty = lower_type(&TypeExpr::Base("bool".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::Bool));
    }

    #[test]
    fn lower_dual_is_polarity_only() {
        // dual(e) denotes the same witness; polarity is handled by checkers.
        let out = lower_str("dual(42)");
        assert_eq!(out[0].1, Term::Var("$int_42".into()));
    }

    #[test]
    fn lower_interaction_with_named_consumer_is_cut() {
        let out = lower_str("42 @ k");
        assert_eq!(
            out[0].1,
            Term::Mu(
                "k".into(),
                Box::new(Command::Cut(Term::Var("$int_42".into()), CoTerm::Covar("k".into())))
            )
        );
    }

    #[test]
    fn lower_interaction_with_compound_consumer_applies_it() {
        let out = lower_str("42 @ fn(x: +i32) -> i32 { x }");
        assert!(matches!(
            &out[0].1,
            Term::Mu(name, command)
                if matches!(
                    command.as_ref(),
                    Command::Cut(
                        Term::Lam(param, body),
                        CoTerm::CoLam(co_param, _)
                    ) if name == "__interaction"
                        && param == "x"
                        && co_param == "__f"
                        && matches!(&**body, Term::Var(value) if value == "x")
                )
        ));
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
