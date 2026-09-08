//! Lowering from surface AST to λ̄μμ̃ core IR.

use crate::ast::*;
use slc_core::command::Command;
use slc_core::coterm::CoTerm;
use slc_core::term::Term;
use slc_core::types::{Base, Type};

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
            let b = lower_expr(body)?;
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
            for a in args {
                let arg = lower_expr(a)?;
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

        Expr::Let { name, value, body } => {
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

        Expr::BinOp { op: _, lhs: _, rhs: _ } => {
            Err(LowerError::Unsupported("binary operators".into()))
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
        Expr::ErrorProp { expr } => {
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
                let b = lower_expr(&arm.body)?;
                arm_terms.push(Term::Lam("__match_arg".into(), Box::new(b)));
            }
            // Build: μmatch. ⟨ __match_dispatch ∥ λ̄__f. ⟨ (s ⊗ arm1 ⊗ ...) ∥ match ⟩ ⟩
            let mut payload = s;
            for a in arm_terms {
                payload = Term::Pair(Box::new(payload), Box::new(a));
            }
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

        Expr::Block(exprs) => {
            // A block evaluates expressions in order. A trailing `let`
            // scopes over the rest of the block, so fold from the end:
            // `let x = v; rest` becomes `let x = v in rest`.
            let mut acc: Option<Term> = None;
            let mut seq_counter = 0;
            for e in exprs.iter().rev() {
                acc = Some(match (&e.kind, acc) {
                    // Bodyless let followed by the rest: scope the rest
                    (Expr::Let { name, value, body: None }, Some(rest)) => {
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
    for d in &p.decls {
        match &d.kind {
            Decl::Fn { name, params, return_type: _, body } => {
                // Multi-param fn: nest lambdas
                let mut term = lower_expr(body)?;
                for p in params.iter().rev() {
                    term = Term::Lam(p.name.clone(), Box::new(term));
                }
                out.push((name.clone(), term));
            }
            Decl::Command { name, params, body } => {
                // command f(x, to k) { E } → κx. μα. E
                let mut term = lower_expr(body)?;
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
            Decl::Struct { .. } | Decl::Enum { .. } => {
                // Type declarations are handled by the checker, not lowering
            }
        }
    }
    Ok(out)
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
    fn lower_types() {
        let ty = lower_type(&TypeExpr::Base("i32".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::I32));
        let ty = lower_type(&TypeExpr::Base("bool".into())).unwrap();
        assert_eq!(ty, Type::Pos(Base::Bool));
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
