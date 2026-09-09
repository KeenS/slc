//! Expression type checking for the ergonomic surface syntax.
//!
//! This pass is intentionally concrete: it validates boolean operators,
//! numeric/comparison operators, indexing, patterns, guards, and annotated
//! bindings using known literal and declaration types. It is not a
//! replacement for the core inference pass; it catches the surface-syntax
//! mistakes that used to appear only at runtime.

use slc_core::types::{Base, Type};
use slc_syntax::ast::{Decl, Expr, Node, Program};
use slc_syntax::lower::lower_type;
use slc_syntax::token::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

pub fn check_program(p: &Program) -> Result<(), Vec<Diagnostic>> {
    let constants = constant_types(p);
    let functions = function_types(p);
    let mut diags = Vec::new();
    let mut env = Env::root(&constants, &functions);
    for d in &p.decls {
        check_decl(d, &mut env, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

#[derive(Debug, Clone)]
struct Env<'a> {
    constants: &'a HashMap<String, Type>,
    functions: &'a HashMap<String, FunctionSignature>,
    locals: Vec<HashMap<String, Type>>,
    continuation_scope_depth: usize,
}

impl<'a> Env<'a> {
    fn root(
        constants: &'a HashMap<String, Type>,
        functions: &'a HashMap<String, FunctionSignature>,
    ) -> Self {
        Self { constants, functions, locals: Vec::new(), continuation_scope_depth: 0 }
    }

    fn push(&mut self) {
        self.locals.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.locals.pop();
    }

    fn push_continuation_scope(&mut self) {
        self.push();
        self.continuation_scope_depth += 1;
    }

    fn pop_continuation_scope(&mut self) {
        self.pop();
        self.continuation_scope_depth -= 1;
    }

    fn define(&mut self, name: &str, ty: Type) {
        if let Some(frame) = self.locals.last_mut() {
            frame.insert(name.to_string(), ty);
        }
    }

    fn lookup(&self, name: &str) -> Option<Type> {
        for frame in self.locals.iter().rev() {
            if let Some(ty) = frame.get(name) {
                return Some(ty.clone());
            }
        }
        self.constants.get(name).cloned()
    }

    fn current_continuation_names(&self) -> Vec<(String, Type)> {
        let depth = self.continuation_scope_depth;
        self.locals[..depth]
            .last()
            .map(|frame| {
                frame
                    .iter()
                    .filter(|(_, ty)| matches!(ty, Type::Neg(_) | Type::Par(..) | Type::Bottom))
                    .map(|(name, ty)| (name.clone(), ty.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }
}

fn constant_types(p: &Program) -> HashMap<String, Type> {
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

#[derive(Debug)]
pub struct FunctionSignature {
    pub params: Vec<Type>,
    pub result: Option<Type>,
}

fn is_builtin(name: &str) -> bool {
    builtin_functions().iter().any(|(builtin, _, _)| *builtin == name)
}

fn builtin_functions() -> Vec<(&'static str, Vec<Type>, Option<Type>)> {
    use Base::*;
    let i64 = Type::Pos(I64);
    let string = Type::Pos(Str);
    let bool_ = Type::Pos(Bool);
    vec![
        ("println", vec![], Some(Type::One)),
        ("print", vec![], Some(Type::One)),
        ("add", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        ("sub", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        ("mul", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        ("div", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        ("rem", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        ("eq", vec![Type::One, Type::One], Some(bool_.clone())),
        ("ne", vec![Type::One, Type::One], Some(bool_.clone())),
        ("lt", vec![Type::One, Type::One], Some(bool_.clone())),
        ("gt", vec![Type::One, Type::One], Some(bool_.clone())),
        ("le", vec![Type::One, Type::One], Some(bool_.clone())),
        ("ge", vec![Type::One, Type::One], Some(bool_.clone())),
        ("str_len", vec![string.clone()], Some(i64.clone())),
        ("str_concat", vec![string.clone(), string.clone()], Some(string.clone())),
        ("is_digit", vec![Type::Pos(Char)], Some(bool_.clone())),
        ("is_ws", vec![Type::Pos(Char)], Some(bool_.clone())),
        ("skip_ws", vec![string.clone(), i64.clone()], Some(i64.clone())),
        ("skip_digits", vec![string.clone(), i64.clone()], Some(i64.clone())),
        ("substring", vec![string.clone(), i64.clone(), i64.clone()], Some(string.clone())),
        ("list_new", vec![], Some(Type::List(Box::new(Type::Pos(Base::I64))))),
        (
            "list_push",
            vec![Type::List(Box::new(Type::Pos(Base::I64))), Type::Pos(Base::I64)],
            Some(Type::List(Box::new(Type::Pos(Base::I64)))),
        ),
        ("char_at", vec![string.clone(), i64.clone()], Some(Type::Pos(Char))),
    ]
}

fn function_types(p: &Program) -> HashMap<String, FunctionSignature> {
    let mut out = builtin_functions()
        .into_iter()
        .map(|(name, params, result)| (name.to_string(), FunctionSignature { params, result }))
        .collect::<HashMap<_, _>>();
    for d in &p.decls {
        let signature = match &d.kind {
            Decl::Fn { name, params, return_type, .. } => {
                Some((name, params, return_type.as_ref().and_then(|ty| lower_type(ty).ok())))
            }
            Decl::Mu { name, params, .. } => Some((name, params, Some(Type::Bottom))),
            _ => None,
        };
        if let Some((name, params, result)) = signature {
            out.insert(
                name.clone(),
                FunctionSignature {
                    params: params.iter().map(|p| lower_type(&p.ty).unwrap_or(Type::One)).collect(),
                    result,
                },
            );
        }
    }
    out
}

fn check_decl(d: &Node<Decl>, env: &mut Env, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { params, body, .. } | Decl::Mu { params, body, .. } => {
            env.push();
            for p in params {
                if let Ok(ty) = lower_type(&p.ty) {
                    env.define(&p.name, ty);
                }
            }
            check_expr(body, env, diags);
            env.pop();
        }
        Decl::Const { name, ty, value } => {
            if !is_constant_initializer(&value.kind, env) {
                diags.push(Diagnostic {
                    message: format!(
                        "const `{name}` initializer must be a literal or another constant"
                    ),
                    span: value.span,
                });
            }
            let expected = lower_type(ty);
            let actual = infer_expr(value, env, diags);
            if let (Ok(expected), Some(actual)) = (&expected, actual)
                && &actual != expected
            {
                diags.push(Diagnostic {
                    message: format!(
                        "const `{name}` is annotated as {expected}; initializer has type {actual}"
                    ),
                    span: value.span,
                });
            }
        }
        _ => {}
    }
}

fn is_constant_initializer(e: &Expr, env: &Env) -> bool {
    match e {
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Char(_) | Expr::Bool(_) => true,
        Expr::Ident(name) => env.constants.contains_key(name),
        _ => false,
    }
}

fn literal_type(e: &Expr) -> Option<Type> {
    Some(match e {
        Expr::Int(_) => Type::Pos(Base::I64),
        Expr::Float(_) => Type::One,
        Expr::Str(_) => Type::Pos(Base::Str),
        Expr::Char(_) => Type::Pos(Base::Char),
        Expr::Bool(_) => Type::Pos(Base::Bool),
        _ => return None,
    })
}

fn pattern_type(pattern: &slc_syntax::ast::Pattern) -> Option<Type> {
    use slc_syntax::ast::Pattern;
    Some(match pattern {
        Pattern::Int(_) => Type::Pos(Base::I64),
        Pattern::Str(_) => Type::Pos(Base::Str),
        Pattern::Char(_) => Type::Pos(Base::Char),
        Pattern::Bool(_) => Type::Pos(Base::Bool),
        Pattern::Float(_) => Type::One,
        Pattern::Range { start, .. } => pattern_type(start)?,
        Pattern::Or(alternatives) => {
            let first = pattern_type(alternatives.first()?)?;
            if alternatives.iter().all(|p| pattern_type(p) == Some(first.clone())) {
                first
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

fn check_pattern(
    pattern: &slc_syntax::ast::Pattern,
    expected: &Type,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use slc_syntax::ast::Pattern;
    let actual = pattern_type(pattern);
    if let Some(actual) = actual
        && &actual != expected
    {
        diags.push(Diagnostic {
            message: format!("pattern has type {actual}; scrutinee has type {expected}"),
            span,
        });
    }
    match pattern {
        Pattern::Or(alternatives) => {
            for alternative in alternatives {
                check_pattern(alternative, expected, span, diags);
            }
        }
        Pattern::Range { start, end } => {
            check_pattern(start, expected, span, diags);
            check_pattern(end, expected, span, diags);
        }
        Pattern::Binding { pattern, .. } => check_pattern(pattern, expected, span, diags),
        Pattern::Tuple(items) => {
            for item in items {
                check_pattern(item, expected, span, diags);
            }
        }
        Pattern::List { items, rest } => {
            let expected = match expected {
                Type::List(item) => (**item).clone(),
                _ => {
                    diags.push(Diagnostic {
                        message: format!("list pattern expects a list, found {expected}"),
                        span,
                    });
                    Type::One
                }
            };
            for item in items {
                check_pattern(item, &expected, span, diags);
            }
            if let Some(rest) = rest {
                check_pattern(rest, &Type::List(Box::new(expected.clone())), span, diags);
            }
        }
        _ => {}
    }
}

fn expected_index_type(value_ty: &Type) -> Option<Type> {
    match value_ty {
        Type::Pos(Base::Str) | Type::List(_) => Some(Type::Pos(Base::I64)),
        _ => None,
    }
}

fn index_result_type(value_ty: &Type) -> Option<Type> {
    match value_ty {
        Type::Pos(Base::Str) => Some(Type::Pos(Base::Char)),
        Type::List(inner) => Some((**inner).clone()),
        _ => None,
    }
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Pos(Base::I32 | Base::I64 | Base::U32 | Base::U64))
}

fn is_comparable(ty: &Type) -> bool {
    is_numeric(ty)
        || matches!(ty, Type::Pos(Base::Char) | Type::Pos(Base::Str) | Type::Pos(Base::Bool))
}

fn infer_expr(e: &Node<Expr>, env: &mut Env, diags: &mut Vec<Diagnostic>) -> Option<Type> {
    check_expr(e, env, diags)
}

fn check_expr(e: &Node<Expr>, env: &mut Env, diags: &mut Vec<Diagnostic>) -> Option<Type> {
    if let Some(ty) = literal_type(&e.kind) {
        return Some(ty);
    }
    match &e.kind {
        Expr::Ident(name) => env.lookup(name),
        Expr::Lambda { param, param_type, body, .. } => {
            env.push();
            if let Some(ty) = param_type
                && let Ok(ty) = lower_type(ty)
            {
                env.define(param, ty);
            }
            let result = check_expr(body, env, diags);
            env.pop();
            result
        }
        Expr::Mu { binder, return_type, body } => {
            env.push_continuation_scope();
            if let Some((name, Some(ty))) = binder
                && let Ok(ty) = lower_type(ty)
            {
                env.define(name, ty);
            }
            let result = check_expr(body, env, diags);
            env.pop_continuation_scope();
            return_type.as_ref().and_then(|ty| lower_type(ty).ok()).or(result)
        }
        Expr::Call { callee, args } => {
            check_expr(callee, env, diags);
            for arg in args {
                check_expr(arg, env, diags);
            }
            if let Expr::Ident(name) = &callee.kind
                && let Some(signature) = env.functions.get(name)
            {
                if is_builtin(name) {
                    let params = &signature.params;
                    for (arg, param) in args.iter().zip(params.iter()) {
                        if let Some(actual) = check_expr(arg, env, diags)
                            && &actual != param
                            && !matches!(param, Type::One)
                        {
                            diags.push(Diagnostic {
                                message: format!(
                                    "argument to `{name}` has type {actual}; expected {param}"
                                ),
                                span: arg.span,
                            });
                        }
                    }
                }
                return signature.result.clone();
            }
            None
        }
        Expr::If { cond, then, otherwise } => {
            let cond_ty = check_expr(cond, env, diags);
            if cond_ty != Some(Type::Pos(Base::Bool)) {
                diags.push(Diagnostic {
                    message: format!(
                        "`if` condition has type {}; expected +bool",
                        cond_ty.map(|ty| ty.to_string()).unwrap_or_else(|| "unknown".into())
                    ),
                    span: cond.span,
                });
            }
            let then_ty = check_expr(then, env, diags);
            if let Some(otherwise) = otherwise {
                let else_ty = check_expr(otherwise, env, diags);
                if let (Some(then_ty), Some(else_ty)) = (then_ty.clone(), else_ty)
                    && then_ty != else_ty
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "`if` branches have incompatible types {then_ty} and {else_ty}"
                        ),
                        span: otherwise.span,
                    });
                }
            }
            then_ty
        }
        Expr::Let { name, ty, value, body } => {
            let actual = check_expr(value, env, diags);
            let annotation = ty.as_ref().and_then(|ty| lower_type(ty).ok());
            if let (Some(annotation), Some(actual)) = (&annotation, actual.clone())
                && &actual != annotation
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`let {name}` is annotated as {annotation}; initializer has type {actual}"
                    ),
                    span: value.span,
                });
            }
            let binding_ty = annotation.or(actual).unwrap_or(Type::One);
            env.push();
            env.define(name, binding_ty);
            let result = body.as_ref().and_then(|body| check_expr(body, env, diags));
            env.pop();
            result
        }
        Expr::BinOp { op, lhs, rhs } => {
            let lhs_ty = check_expr(lhs, env, diags);
            let rhs_ty = check_expr(rhs, env, diags);
            match op {
                slc_syntax::ast::BinOp::And | slc_syntax::ast::BinOp::Or => {
                    for (operand, ty) in [(lhs, lhs_ty.clone()), (rhs, rhs_ty.clone())] {
                        if ty != Some(Type::Pos(Base::Bool)) {
                            diags.push(Diagnostic {
                                message: format!(
                                    "boolean operand has type {}; expected +bool",
                                    ty.map(|ty| ty.to_string()).unwrap_or_else(|| "unknown".into())
                                ),
                                span: operand.span,
                            });
                        }
                    }
                    Some(Type::Pos(Base::Bool))
                }
                slc_syntax::ast::BinOp::Add
                | slc_syntax::ast::BinOp::Sub
                | slc_syntax::ast::BinOp::Mul
                | slc_syntax::ast::BinOp::Div
                | slc_syntax::ast::BinOp::Mod => {
                    if let (Some(lhs_ty), Some(rhs_ty)) = (lhs_ty.clone(), rhs_ty.clone()) {
                        let string_add = matches!(
                            (op, lhs_ty.clone(), rhs_ty.clone()),
                            (
                                slc_syntax::ast::BinOp::Add,
                                Type::Pos(Base::Str),
                                Type::Pos(Base::Str)
                            )
                        );
                        if string_add {
                            return Some(Type::Pos(Base::Str));
                        }
                        if lhs_ty != rhs_ty || !is_numeric(&lhs_ty) || !is_numeric(&rhs_ty) {
                            diags.push(Diagnostic {
                                message: format!(
                                    "arithmetic operands have types {lhs_ty} and {rhs_ty}"
                                ),
                                span: e.span,
                            });
                        }
                    }
                    lhs_ty.or(rhs_ty)
                }
                slc_syntax::ast::BinOp::Eq
                | slc_syntax::ast::BinOp::Ne
                | slc_syntax::ast::BinOp::Lt
                | slc_syntax::ast::BinOp::Gt
                | slc_syntax::ast::BinOp::Le
                | slc_syntax::ast::BinOp::Ge => {
                    if let (Some(lhs_ty), Some(rhs_ty)) = (lhs_ty, rhs_ty)
                        && (lhs_ty != rhs_ty || !is_comparable(&lhs_ty) || !is_comparable(&rhs_ty))
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "comparison operands have types {lhs_ty} and {rhs_ty}"
                            ),
                            span: e.span,
                        });
                    }
                    Some(Type::Pos(Base::Bool))
                }
            }
        }
        Expr::UnOp { op, body } => {
            let body_ty = check_expr(body, env, diags);
            match op {
                slc_syntax::ast::UnOp::Not => {
                    if body_ty != Some(Type::Pos(Base::Bool)) {
                        diags.push(Diagnostic {
                            message: format!(
                                "`!` operand has type {}; expected +bool",
                                body_ty
                                    .map(|ty| ty.to_string())
                                    .unwrap_or_else(|| "unknown".into())
                            ),
                            span: body.span,
                        });
                    }
                    Some(Type::Pos(Base::Bool))
                }
                slc_syntax::ast::UnOp::Neg => {
                    if body_ty.as_ref().is_some_and(|ty| !is_numeric(ty)) {
                        diags.push(Diagnostic {
                            message: format!(
                                "unary `-` operand has type {body_ty:?}; expected numeric"
                            ),
                            span: body.span,
                        });
                    }
                    body_ty
                }
            }
        }
        Expr::Index { value, index } => {
            let value_ty = check_expr(value, env, diags);
            let index_ty = check_expr(index, env, diags);
            if let Some(value_ty) = value_ty.clone() {
                match expected_index_type(&value_ty) {
                    Some(expected_index) => {
                        if index_ty != Some(expected_index.clone()) {
                            diags.push(Diagnostic {
                                message: format!(
                                    "index has type {}; expected {expected_index}",
                                    index_ty
                                        .map(|ty| ty.to_string())
                                        .unwrap_or_else(|| "unknown".into())
                                ),
                                span: index.span,
                            });
                        }
                    }
                    None => {
                        diags.push(Diagnostic {
                            message: format!("type {value_ty} is not indexable"),
                            span: value.span,
                        });
                    }
                }
                index_result_type(&value_ty)
            } else {
                None
            }
        }
        Expr::Slice { value, start, end } => {
            let value_ty = check_expr(value, env, diags);
            for endpoint in [start, end].into_iter().flatten() {
                let endpoint_ty = check_expr(endpoint, env, diags);
                if endpoint_ty != Some(Type::Pos(Base::I64)) {
                    diags.push(Diagnostic {
                        message: format!(
                            "range endpoint has type {}; expected +i64",
                            endpoint_ty
                                .map(|ty| ty.to_string())
                                .unwrap_or_else(|| "unknown".into())
                        ),
                        span: endpoint.span,
                    });
                }
            }
            if let Some(Type::List(inner)) = value_ty.clone() {
                Some(Type::List(inner))
            } else {
                value_ty
            }
        }
        Expr::Match { scrutinee, arms } => {
            let scrutinee_ty = check_expr(scrutinee, env, diags);
            if let Some(scrutinee_ty) = &scrutinee_ty {
                for arm in arms {
                    check_pattern(&arm.pattern, scrutinee_ty, arm.body.span, diags);
                }
            }
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    let guard_ty = check_expr(guard, env, diags);
                    if guard_ty != Some(Type::Pos(Base::Bool)) {
                        diags.push(Diagnostic {
                            message: format!(
                                "match guard has type {}; expected +bool",
                                guard_ty
                                    .map(|ty| ty.to_string())
                                    .unwrap_or_else(|| "unknown".into())
                            ),
                            span: guard.span,
                        });
                    }
                }
                check_expr(&arm.body, env, diags);
            }
            None
        }
        Expr::Pair(items) => items
            .iter()
            .map(|item| check_expr(item, env, diags))
            .collect::<Option<Vec<_>>>()
            .map(|types| {
                types.into_iter().rev().reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
            })?,
        Expr::Block(exprs) => {
            let mut result = None;
            env.push();
            for expr in exprs {
                if let Expr::Let { name, ty, value, body: None } = &expr.kind {
                    let actual = check_expr(value, env, diags);
                    let annotation = ty.as_ref().and_then(|ty| lower_type(ty).ok());
                    if let (Some(annotation), Some(actual)) = (&annotation, actual.clone())
                        && &actual != annotation
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "`let {name}` is annotated as {annotation}; initializer has type {actual}"
                            ),
                            span: value.span,
                        });
                    }
                    env.define(name, annotation.or(actual).unwrap_or(Type::One));
                } else {
                    result = check_expr(expr, env, diags);
                }
            }
            env.pop();
            result
        }
        Expr::ErrorProp { expr, continuation } => {
            check_expr(expr, env, diags);
            match continuation {
                Some(name) => {
                    if env.lookup(name).is_none() {
                        diags.push(Diagnostic {
                            message: format!("`?{name}` refers to unknown continuation `{name}`"),
                            span: e.span,
                        });
                    }
                }
                None => {
                    if env.current_continuation_names().is_empty() {
                        diags.push(Diagnostic {
                            message: "`?` requires a current error continuation".into(),
                            span: e.span,
                        });
                    }
                }
            }
            None
        }
        Expr::Dual { body } => check_expr(body, env, diags),
        Expr::Interaction { left, right } => {
            check_expr(left, env, diags);
            check_expr(right, env, diags)
        }
        Expr::Service { agent, continuations } => {
            check_expr(agent, env, diags);
            for continuation in continuations {
                check_expr(continuation, env, diags);
            }
            None
        }
        Expr::Job { agent, values } => {
            check_expr(agent, env, diags);
            for value in values {
                check_expr(value, env, diags);
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slc_syntax::lexer::lex;
    use slc_syntax::parser::parse;

    fn check(s: &str) -> Result<(), Vec<Diagnostic>> {
        let toks = lex(s).unwrap();
        let prog = parse(toks).unwrap();
        check_program(&prog)
    }

    #[test]
    fn boolean_operators_ok() {
        assert!(check("fn f(a: +bool, b: +bool) -> bool { a && b || !a }").is_ok());
    }

    #[test]
    fn boolean_operand_mismatch_rejected() {
        let diags = check("fn f(a: +i32) -> bool { a && true }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("boolean operand")));
        assert!(diags[0].span.start > 0);
    }

    #[test]
    fn arithmetic_type_mismatch_rejected() {
        let diags = check("fn f(a: +i32, b: +i64) -> i64 { a + b }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("arithmetic operands")));
    }

    #[test]
    fn comparison_char_and_int_mismatch_rejected() {
        let diags = check("fn f(a: +char, b: +i32) -> bool { a < b }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("comparison operands")));
    }

    #[test]
    fn char_comparison_ok() {
        assert!(check("fn f(a: +char, b: +char) -> bool { a < b }").is_ok());
    }

    #[test]
    fn string_concat_ok() {
        assert!(check(r#"fn f(a: +String, b: +String) -> String { a + b }"#).is_ok());
    }

    #[test]
    fn guard_must_be_bool() {
        let diags = check("fn f(c: +i64) -> i64 { match c { _ if c => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("match guard")));
    }

    #[test]
    fn char_pattern_checked() {
        let diags = check("fn f(c: +char) -> i64 { match c { 3 => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type")));
    }

    #[test]
    fn index_type_checked() {
        let diags = check("fn f(s: +String, i: +bool) -> char { s[i] }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("index has type")));
    }

    #[test]
    fn list_index_checked() {
        assert!(check("fn f(l: [+i32], i: +i64) -> i32 { l[i] }").is_ok());
        let diags = check("fn f(l: [+i32], i: +bool) -> i32 { l[i] }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("index has type")));
    }

    #[test]
    fn typed_let_checked() {
        let diags = check("fn f() -> i32 { let x: +char = 1; 2 }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("annotated")));
    }

    #[test]
    fn const_type_checked() {
        let diags = check("const X: +char = 1;").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("initializer")));
    }

    #[test]
    fn error_prop_without_continuation_rejected() {
        let diags = check("fn f() -> i32 { read_file(\"x\")? }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`?` requires")));
    }

    #[test]
    fn named_error_continuation_checked() {
        let diags = check("fn f() -> i32 { read_file(\"x\")?missing }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("unknown continuation")));
    }

    #[test]
    fn non_constant_const_initializer_rejected() {
        let diags = check("const X: +i32 = add(1, 2);").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("literal or another constant")));
    }

    #[test]
    fn range_endpoints_checked() {
        let diags = check("fn f(c: +char) -> i64 { match c { 'a'..=3 => 1 } }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type")));
    }
}
