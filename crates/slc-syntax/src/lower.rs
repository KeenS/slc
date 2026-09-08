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
        Expr::Int(n) => Ok(Term::Var(format!("int_{n}"))),
        Expr::Float(n) => Ok(Term::Var(format!("float_{n}"))),
        Expr::Str(s) => Ok(Term::Var(format!("str_{s:?}"))),
        Expr::Char(c) => Ok(Term::Var(format!("char_{c}"))),
        Expr::Bool(b) => Ok(Term::Var(if *b { "true" } else { "false" }.to_string())),
        Expr::Ident(s) => Ok(Term::Var(s.clone())),

        Expr::Lambda { param, param_type: _, return_type: _, body } => {
            // fn(x) { body } → λx. body'
            let b = lower_expr(body)?;
            Ok(Term::Lam(param.clone(), Box::new(b)))
        }

        Expr::Mu { binder, return_type: _, body } => {
            // mu(k: -T) { body } → μα. ⟨ body' ∥ α ⟩
            let b = lower_expr(body)?;
            let a = binder.as_ref().map(|(n, _)| n.clone()).unwrap_or_else(|| "k".into());
            Ok(Term::Mu(a.clone(), Box::new(Command::Cut(b, CoTerm::Covar(a)))))
        }

        Expr::Call { callee, args } => {
            // f(a) → ⟨ f' ∥ μ̃x. ... ⟩ (simplified: f' applied to args)
            let _ = args;
            lower_expr(callee)
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
            // let x = v; body → ⟨ λx. body' ∥ μ̃x. v' ⟩ (via application)
            let v = lower_expr(value)?;
            let b = body
                .as_ref()
                .map(|b| lower_expr(b))
                .transpose()?
                .unwrap_or_else(|| Term::Var("unit".into()));
            // (λx. b) v
            let lam = Term::Lam(name.clone(), Box::new(b));
            Ok(Term::Mu(
                "let".into(),
                Box::new(Command::Cut(
                    lam,
                    CoTerm::MuTilde(
                        name.clone(),
                        Box::new(Command::Cut(v, CoTerm::Covar("let".into()))),
                    ),
                )),
            ))
        }

        Expr::If { cond, then, otherwise } => {
            // if c { t } else { e } → match c { true => t', false => e' }
            let c = lower_expr(cond)?;
            let t = lower_expr(then)?;
            let e = otherwise
                .as_ref()
                .map(|e| lower_expr(e))
                .transpose()?
                .unwrap_or_else(|| Term::Var("unit".into()));
            // Encode as sum: inl(t) if true, inr(e) if false
            let _ = c;
            Ok(Term::Inl(Box::new(t.clone())).join_or(Term::Inr(Box::new(e))))
        }

        Expr::BinOp { op: _, lhs: _, rhs: _ } => {
            Err(LowerError::Unsupported("binary operators".into()))
        }

        Expr::Interaction { left: _, right: _ } => {
            Err(LowerError::Unsupported("@ interaction".into()))
        }
        Expr::Spawn { body: _ } => Err(LowerError::Unsupported("spawn".into())),
        Expr::Dual { body: _ } => Err(LowerError::Unsupported("dual".into())),
        Expr::ErrorProp { expr: _ } => Err(LowerError::Unsupported("?".into())),

        Expr::Match { scrutinee: _, arms: _ } => Err(LowerError::Unsupported("match".into())),

        Expr::CommandDef { name: _, params: _, body: _ } => {
            Err(LowerError::Unsupported("command expressions".into()))
        }
    }
}

trait JoinOr {
    fn join_or(self, other: Term) -> Term;
}

impl JoinOr for Term {
    fn join_or(self, other: Term) -> Term {
        // This is a simplification; real lowering uses sum types
        Term::Pair(Box::new(self), Box::new(other))
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
        assert_eq!(out[0].1, Term::Var("int_42".into()));
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
