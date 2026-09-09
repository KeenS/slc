//! Bidirectional inference for surface declarations.
//!
//! This is deliberately conservative: annotations are checked, unannotated
//! ports are represented by core type variables, and the final result must
//! contain no unresolved variables. Generic declarations allocate one core
//! variable per declared type parameter, preserving scoping without yet
//! committing to Rust-style monomorphization.

use slc_core::types::Type;
use slc_core::typing::{TypeError, Unification};
use slc_syntax::ast::{Decl, Node, Program};
use slc_syntax::lower::{LowerError, lower_type};
use slc_syntax::token::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

#[derive(Debug)]
pub enum InferenceError {
    Lower(LowerError),
    Type(TypeError),
    Diag(Vec<Diagnostic>),
}

impl From<LowerError> for InferenceError {
    fn from(e: LowerError) -> Self {
        Self::Lower(e)
    }
}

impl From<TypeError> for InferenceError {
    fn from(e: TypeError) -> Self {
        Self::Type(e)
    }
}

pub fn infer_program(p: &Program) -> Result<Vec<DeclarationType>, Vec<Diagnostic>> {
    let mut out = Vec::new();
    let mut diags = Vec::new();
    for d in &p.decls {
        match infer_decl(d) {
            Ok(ty) => out.push(ty),
            Err(e) => diags.push(Diagnostic { message: format!("inference: {e}"), span: d.span }),
        }
    }
    if diags.is_empty() { Ok(out) } else { Err(diags) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeclarationType {
    pub name: String,
    pub ty: Type,
}

fn infer_decl(d: &Node<Decl>) -> Result<DeclarationType, InferenceError> {
    match &d.kind {
        Decl::Fn { name, type_params, params, return_type, body: _ } => {
            let mut u = Unification::new();
            let mut vars: HashMap<&str, Type> = HashMap::new();
            for tp in type_params {
                vars.insert(tp, u.fresh_var());
            }

            let mut inputs = Vec::new();
            for p in params {
                let ty = if let slc_syntax::ast::TypeExpr::Positive(inner) = &p.ty
                    && let slc_syntax::ast::TypeExpr::Base(name) = &inner.kind
                    && let Some(ty) = vars.get(name.as_str())
                {
                    ty.clone()
                } else {
                    lower_type(&p.ty)?
                };
                let ty = u.apply(&ty);
                inputs.push(u.unify_with_polarity(&ty, &ty, !p.is_continuation)?);
            }

            let output = match return_type {
                Some(ty) => {
                    let ty = if let slc_syntax::ast::TypeExpr::Base(name) = ty
                        && let Some(ty) = vars.get(name.as_str())
                    {
                        ty.clone()
                    } else {
                        lower_type(ty)?
                    };
                    u.apply(&ty)
                }
                None => u.fresh_var(),
            };

            let ty = inputs
                .into_iter()
                .rev()
                .fold(output, |acc, input| Type::Fun(Box::new(input), Box::new(acc)));
            let ty = if type_params.is_empty() {
                u.resolve_or_cannot_infer(&ty, &format!("fn {name}"))?
            } else {
                ty
            };
            Ok(DeclarationType { name: name.clone(), ty })
        }
        Decl::Command { name, params, body: _ } => {
            let mut u = Unification::new();
            let mut inputs = Vec::new();
            for p in params {
                let ty = lower_type(&p.ty)?;
                inputs.push(u.unify_with_polarity(&ty, &ty, !p.is_continuation)?);
            }
            let output = Type::Bottom;
            let ty = inputs
                .into_iter()
                .rev()
                .fold(output, |acc, input| Type::Fun(Box::new(input), Box::new(acc)));
            let ty = u.resolve_or_cannot_infer(&ty, &format!("command {name}"))?;
            Ok(DeclarationType { name: name.clone(), ty })
        }
        Decl::Struct { name, .. } | Decl::Enum { name, .. } => {
            Ok(DeclarationType { name: name.clone(), ty: Type::One })
        }
        Decl::Const { name, ty, .. } => {
            Ok(DeclarationType { name: name.clone(), ty: lower_type(ty)? })
        }
    }
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceError::Lower(e) => write!(f, "{e}"),
            InferenceError::Type(e) => write!(f, "{e}"),
            InferenceError::Diag(diags) => {
                write!(
                    f,
                    "{}",
                    diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_core::types::Base;
    use slc_syntax::lexer::lex;
    use slc_syntax::parser::parse;

    fn infer(s: &str) -> Result<Vec<DeclarationType>, Vec<Diagnostic>> {
        let p = parse(lex(s).unwrap()).unwrap();
        infer_program(&p)
    }

    #[test]
    fn fn_annotation_infers_function_type() {
        let out = infer("fn id(x: +i32) -> i32 { x }").unwrap();
        assert_eq!(
            out[0].ty,
            Type::Fun(Box::new(Type::Pos(Base::I32)), Box::new(Type::Pos(Base::I32)))
        );
    }

    #[test]
    fn command_infers_parametric_type() {
        let out = infer("command step(x: +i32, to k: -i32) { k(x) }").unwrap();
        assert_eq!(
            out[0].ty,
            Type::Fun(
                Box::new(Type::Pos(Base::I32)),
                Box::new(Type::Fun(Box::new(Type::Neg(Base::I32)), Box::new(Type::Bottom)))
            )
        );
    }

    #[test]
    fn generic_type_variables_are_supported() {
        let out = infer("fn id<T>(x: +T) -> T { x }").unwrap();
        assert_eq!(out[0].ty, Type::Fun(Box::new(Type::Var(0)), Box::new(Type::Var(0))));
    }

    #[test]
    fn missing_return_cannot_infer() {
        let out = infer("fn f(x: +i32) { x }");
        assert!(out.is_err());
        assert!(out.unwrap_err()[0].message.contains("cannot infer"));
    }

    #[test]
    fn polarity_constraint_rejects_wrong_polarity() {
        let out = infer("fn f(x: -i32) { x }");
        assert!(out.is_err());
        assert!(out.unwrap_err()[0].message.contains("positive"));
    }
}
