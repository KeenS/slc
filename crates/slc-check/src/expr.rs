//! Expression type checking for the ergonomic surface syntax.
//!
//! This pass is intentionally concrete: it validates boolean operators,
//! numeric/comparison operators, indexing, patterns, guards, and annotated
//! bindings using known literal and declaration types. It is not a
//! replacement for the core inference pass; it catches the surface-syntax
//! mistakes that used to appear only at runtime.

use slc_core::types::{Base, Type};
use slc_syntax::ast::{Decl, Expr, Named, Node, Program, TypeExpr};
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
    let enums = enum_types(p);
    let mut diags = Vec::new();
    let mut env = Env::root(&constants, &functions);
    for d in &p.decls {
        check_decl(d, &enums, &mut env, &mut diags);
    }
    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

/// What the checker knows about the program's type declarations: the variants
/// of each `enum`, the payload each variant carries, and the fields of each
/// `struct`.
#[derive(Debug, Default)]
pub struct Declarations {
    /// Every declaration name in the program: `struct` and `enum` alike.
    declarations: std::collections::HashSet<String>,
    /// Declaration name → variant names, in declaration order.
    variants: HashMap<String, Vec<String>>,
    /// Fully qualified label → declaration name and payload types.
    signatures: HashMap<String, (String, Vec<Type>)>,
    /// Unqualified variant name → its label, when only one enum declares it.
    unqualified: HashMap<String, Option<String>>,
    /// Declaration name → fields, in declaration order.
    structs: HashMap<String, Vec<(String, Type)>>,
}

impl Declarations {
    /// Lower a written type, resolving a declaration name to its named type.
    /// `lower_type` only knows the built-in types, so `struct` and `enum`
    /// names have to be resolved here — including under a sign or a
    /// connective, so `-ParseResult` is a consumer of a declared type.
    fn resolve(&self, ty: &TypeExpr) -> Option<Type> {
        let resolved = match ty {
            TypeExpr::Base(name) if self.declarations.contains(name) => Type::Named(name.clone()),
            TypeExpr::Positive(inner) => self.resolve(&inner.kind)?,
            TypeExpr::Negative(inner) if !matches!(inner.kind, TypeExpr::Bottom) => {
                self.resolve(&inner.kind)?.dual()
            }
            TypeExpr::Tensor(a, b) => {
                Type::Tensor(Box::new(self.resolve(&a.kind)?), Box::new(self.resolve(&b.kind)?))
            }
            TypeExpr::Par(a, b) => {
                Type::Par(Box::new(self.resolve(&a.kind)?), Box::new(self.resolve(&b.kind)?))
            }
            // `A → B` is `-A ⅋ B`.
            TypeExpr::Fun(a, b) => Type::arrow(self.resolve(&a.kind)?, self.resolve(&b.kind)?),
            TypeExpr::List(inner) => Type::List(Box::new(self.resolve(&inner.kind)?)),
            // `dual(A)` applies the involution; only a declaration's name
            // stays wrapped, because it is opaque to the core.
            TypeExpr::Dual(inner) => self.resolve(&inner.kind)?.dual(),
            other => return lower_type(other).ok(),
        };
        Some(resolved)
    }

    /// The field types of a declared struct, in declaration order.
    fn fields(&self, name: &str) -> Option<Vec<Type>> {
        Some(self.structs.get(name)?.iter().map(|(_, ty)| ty.clone()).collect())
    }

    /// Resolve a variant path or an unambiguous unqualified variant name.
    /// Whether a name is a declared `struct` or `enum`.
    fn declares(&self, name: &str) -> bool {
        self.declarations.contains(name)
    }

    fn variant(&self, name: &str) -> Option<&(String, Vec<Type>)> {
        if let Some(signature) = self.signatures.get(name) {
            return Some(signature);
        }
        let label = self.unqualified.get(name)?.as_ref()?;
        self.signatures.get(label)
    }
}

fn enum_types(p: &Program) -> Declarations {
    let mut enums = Declarations::default();
    for d in &p.decls {
        if let Decl::Struct { name, .. } | Decl::Enum { name, .. } = &d.kind {
            enums.declarations.insert(name.clone());
        }
        if let Decl::Struct { name, fields } = &d.kind {
            enums.structs.insert(
                name.clone(),
                fields
                    .iter()
                    .map(|(field, ty)| (field.clone(), lower_type(ty).unwrap_or(Type::One)))
                    .collect(),
            );
        }
    }
    for d in &p.decls {
        let Decl::Enum { name, variants } = &d.kind else { continue };
        enums.variants.insert(name.clone(), variants.iter().map(|(v, _)| v.clone()).collect());
        for (variant, payload) in variants {
            let label = format!("{name}::{variant}");
            let payload =
                payload.iter().map(|ty| enums.resolve(ty).unwrap_or(Type::One)).collect::<Vec<_>>();
            enums.signatures.insert(label.clone(), (name.clone(), payload));
            enums
                .unqualified
                .entry(variant.clone())
                .and_modify(|existing| *existing = None)
                .or_insert(Some(label));
        }
    }
    enums
}

#[derive(Debug, Clone)]
struct Env<'a> {
    constants: &'a HashMap<String, Type>,
    functions: &'a HashMap<String, FunctionSignature>,
    locals: Vec<HashMap<String, Type>>,
    /// The type the enclosing negative `fn` consumes — what follows its
    /// `<-`. A `select` in its body is the consumer of exactly that, so a
    /// `select` there need not repeat it.
    consumed: Option<Type>,
}

impl<'a> Env<'a> {
    fn root(
        constants: &'a HashMap<String, Type>,
        functions: &'a HashMap<String, FunctionSignature>,
    ) -> Self {
        Self { constants, functions, locals: Vec::new(), consumed: None }
    }

    fn push(&mut self) {
        self.locals.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.locals.pop();
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
        self.locals
            .iter()
            .rev()
            .find_map(|frame| {
                let names: Vec<_> = frame
                    .iter()
                    .filter(|(_, ty)| matches!(ty, Type::Neg(_) | Type::Par(..) | Type::Bottom))
                    .map(|(name, ty)| (name.clone(), ty.clone()))
                    .collect();
                (!names.is_empty()).then_some(names)
            })
            .unwrap_or_default()
    }
}

fn constant_types(p: &Program) -> HashMap<String, Type> {
    // `EXIT` is the top-level continuation, so it is a binding of negative
    // type rather than a function: it is activated with a cut.
    let mut out: HashMap<String, Type> = HashMap::new();
    out.insert("EXIT".into(), Type::Neg(Base::I32));
    out.extend(constant_declarations(p));
    out
}

fn constant_declarations(p: &Program) -> HashMap<String, Type> {
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
    /// Which declared parameters are continuations, positionally. This is the
    /// declaration's continuation row.
    pub continuations: Vec<bool>,
    pub result: Option<Type>,
}

fn is_builtin(name: &str) -> bool {
    builtin_functions().iter().any(|builtin| builtin.name == name)
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
    let unknown = Type::One;

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
        Builtin { name, params, continuations, result: Some(Type::Bottom) }
    };

    vec![
        function("println", vec![], Some(Type::One)),
        function("print", vec![], Some(Type::One)),
        function("add", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("sub", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("mul", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("div", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("rem", vec![i64.clone(), i64.clone()], Some(i64.clone())),
        function("eq", vec![Type::One, Type::One], Some(bool_.clone())),
        function("ne", vec![Type::One, Type::One], Some(bool_.clone())),
        function("lt", vec![Type::One, Type::One], Some(bool_.clone())),
        function("gt", vec![Type::One, Type::One], Some(bool_.clone())),
        function("le", vec![Type::One, Type::One], Some(bool_.clone())),
        function("ge", vec![Type::One, Type::One], Some(bool_.clone())),
        function("str_len", vec![string.clone()], Some(i64.clone())),
        function("str_concat", vec![string.clone(), string.clone()], Some(string.clone())),
        function("int_to_str", vec![i64.clone()], Some(string.clone())),
        function("str_eq", vec![string.clone(), string.clone()], Some(bool_.clone())),
        function("is_digit", vec![char_.clone()], Some(bool_.clone())),
        function("is_ws", vec![char_.clone()], Some(bool_.clone())),
        function("skip_ws", vec![string.clone(), i64.clone()], Some(i64.clone())),
        function("skip_digits", vec![string.clone(), i64.clone()], Some(i64.clone())),
        function("substring", vec![string.clone(), i64.clone(), i64.clone()], Some(string.clone())),
        function("file_exists", vec![string.clone()], Some(bool_.clone())),
        function("list_new", vec![], Some(Type::List(Box::new(i64.clone())))),
        function("list_len", vec![unknown.clone()], Some(i64.clone())),
        function(
            "list_push",
            vec![Type::List(Box::new(i64.clone())), i64.clone()],
            Some(Type::List(Box::new(i64.clone()))),
        ),
        // Parsing, input/output, and lookup can fail or find nothing, so they
        // offer their outcomes to continuations.
        offers(
            "parse_int",
            vec![string.clone()],
            vec![Type::Neg(I64), Type::Neg(Str), Type::Neg(Str)],
        ),
        offers("read_file", vec![string.clone()], vec![Type::Neg(Str), Type::Neg(Str)]),
        offers(
            "write_file",
            vec![string.clone(), string.clone()],
            vec![Type::Neg(Unit), Type::Neg(Str)],
        ),
        offers("char_at", vec![string.clone(), i64.clone()], vec![Type::Neg(Char), Type::Neg(Str)]),
        offers(
            "list_get",
            vec![unknown.clone(), i64.clone()],
            vec![unknown.clone(), Type::Neg(Str)],
        ),
        offers("map_get", vec![unknown.clone(), unknown.clone()], vec![unknown, Type::Neg(Str)]),
        offers("find_char", vec![string, i64.clone(), i64], vec![Type::Neg(I64), Type::Neg(Str)]),
    ]
}

/// A declaration's parameter type. Declarations are interfaces, so one is
/// always written; anything else is unknown to the caller.
fn declared_type(p: &slc_syntax::ast::Param) -> Type {
    p.ty.as_ref().and_then(|ty| lower_type(ty).ok()).unwrap_or(Type::One)
}

fn function_types(p: &Program) -> HashMap<String, FunctionSignature> {
    let mut out = builtin_functions()
        .into_iter()
        .map(|builtin| {
            (
                builtin.name.to_string(),
                FunctionSignature {
                    params: builtin.params,
                    continuations: builtin.continuations,
                    result: builtin.result,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for d in &p.decls {
        let signature = match &d.kind {
            Decl::Fn { name, params, return_type, polarity, .. } => {
                // A negative function produces the *consumer* of the type
                // written after `<-`.
                let result = return_type.as_ref().and_then(|ty| lower_type(ty).ok()).map(|ty| {
                    if *polarity == slc_syntax::ast::FunctionPolarity::Negative {
                        ty.dual()
                    } else {
                        ty
                    }
                });
                Some((name, params, result))
            }
            Decl::Command { name, value_params, continuation_params, .. } => {
                let declared: Vec<_> =
                    value_params.iter().chain(continuation_params.iter()).collect();
                let params = declared.iter().map(|p| declared_type(p)).collect();
                let continuations = declared.iter().map(|p| p.is_continuation).collect();
                out.insert(
                    name.clone(),
                    FunctionSignature { params, continuations, result: Some(Type::Bottom) },
                );
                continue;
            }
            _ => None,
        };
        if let Some((name, params, result)) = signature {
            out.insert(
                name.clone(),
                FunctionSignature {
                    params: params.iter().map(declared_type).collect(),
                    continuations: params.iter().map(|p| p.is_continuation).collect(),
                    result,
                },
            );
        }
    }
    out
}

/// The type a `select`'s arms name, when one of them does: a struct pattern
/// names its struct, `Color::Red(x)` its enum, and a bare `Red` the enum that
/// declares it.
fn named_by_arms(arms: &[slc_syntax::ast::SelectArm], enums: &Declarations) -> Option<Type> {
    arms.iter().find_map(|arm| match arm.pattern.names()? {
        Named::Declaration(name) if enums.declares(name) => Some(Type::Named(name.to_string())),
        Named::Variant(name) => {
            enums.variant(name).map(|(declaration, _)| Type::Named(declaration.clone()))
        }
        Named::Declaration(_) => None,
    })
}

/// The type an unannotated local-`mu` parameter has, read off the body that
/// uses it. Two shapes say it outright: a call that hands the parameter to a
/// slot whose type the callee declares, and a cut that sends a value to it.
fn infer_param_type(
    name: &str,
    body: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
) -> Option<Type> {
    if let Expr::Call { callee, args } = &body.kind
        && let Expr::Ident(function) = &callee.kind
        && let Some(slot) =
            args.iter().position(|a| matches!(&a.kind, Expr::Ident(x) if x == name))
        && let Some(ty) = env.functions.get(function).and_then(|s| s.params.get(slot)).cloned()
        // `Type::One` is what an untyped slot carries; it says nothing.
        && ty != Type::One
    {
        return Some(ty);
    }
    if let Expr::Cut { value, consumer } = &body.kind
        && matches!(&consumer.kind, Expr::Ident(x) if x == name)
    {
        // `v @ k` makes `k` the consumer of whatever `v` is. The value is
        // checked again in place, so these diagnostics are thrown away.
        return check_expr(value, enums, env, &mut Vec::new()).map(|ty| ty.dual());
    }
    body.kind.children().into_iter().find_map(|child| infer_param_type(name, child, enums, env))
}

fn check_decl(d: &Node<Decl>, enums: &Declarations, env: &mut Env, diags: &mut Vec<Diagnostic>) {
    match &d.kind {
        Decl::Fn { params, body, polarity, return_type, .. } => {
            env.push();
            for p in params {
                if let Some(ty) = p.ty.as_ref().and_then(|ty| enums.resolve(ty)) {
                    env.define(&p.name, ty);
                }
            }
            // A negative function produces the consumer of what follows its
            // `<-`, so that is what a `select` in its body consumes.
            let outer = env.consumed.take();
            env.consumed = (*polarity == slc_syntax::ast::FunctionPolarity::Negative)
                .then(|| return_type.as_ref().and_then(|ty| enums.resolve(ty)))
                .flatten();
            check_expr(body, enums, env, diags);
            env.consumed = outer;
            env.pop();
        }
        Decl::Command { value_params, continuation_params, body, .. } => {
            env.push();
            for p in value_params.iter().chain(continuation_params.iter()) {
                if let Some(ty) = p.ty.as_ref().and_then(|ty| enums.resolve(ty)) {
                    env.define(&p.name, ty);
                }
            }
            check_expr(body, enums, env, diags);
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
            let actual = infer_expr(value, enums, env, diags);
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
    declarations: &Declarations,
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
                check_pattern(alternative, expected, declarations, span, diags);
            }
        }
        Pattern::Range { start, end } => {
            check_pattern(start, expected, declarations, span, diags);
            check_pattern(end, expected, declarations, span, diags);
        }
        Pattern::Binding { pattern, .. } => {
            check_pattern(pattern, expected, declarations, span, diags)
        }
        Pattern::Tuple(items) => {
            for item in items {
                check_pattern(item, expected, declarations, span, diags);
            }
        }
        // A struct pattern decomposes the product: the same fields, in the
        // same order, with the same types as the declaration.
        Pattern::Struct { name, fields } => {
            let Some(declared) = declarations.structs.get(name) else {
                diags.push(Diagnostic {
                    message: format!("`{name}` is not a declared struct"),
                    span,
                });
                return;
            };
            if expected != &Type::Named(name.clone()) {
                diags.push(Diagnostic {
                    message: format!(
                        "struct pattern `{name}` cannot match a scrutinee of type {expected}"
                    ),
                    span,
                });
            }
            let written: Vec<&String> = fields.iter().map(|(field, _)| field).collect();
            let expected_fields: Vec<&String> = declared.iter().map(|(field, _)| field).collect();
            if written != expected_fields {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` has fields {}; the pattern writes {}",
                        expected_fields
                            .iter()
                            .map(|f| format!("`{f}`"))
                            .collect::<Vec<_>>()
                            .join(", "),
                        if written.is_empty() {
                            "none".to_string()
                        } else {
                            written.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
                        }
                    ),
                    span,
                });
            }
            for (field, pattern) in fields {
                if let Some((_, field_ty)) = declared.iter().find(|(declared, _)| declared == field)
                {
                    check_pattern(pattern, field_ty, declarations, span, diags);
                }
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
                check_pattern(item, &expected, declarations, span, diags);
            }
            if let Some(rest) = rest {
                check_pattern(
                    rest,
                    &Type::List(Box::new(expected.clone())),
                    declarations,
                    span,
                    diags,
                );
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

/// Does a value written as `expr`, inferred as `actual`, fit a port that
/// requires `expected`?
///
/// An integer literal takes the integer type its port requires — `0 @ EXIT`
/// sends an `i32` — and is `+i64` only when nothing constrains it. Every
/// other value must match its port exactly.
fn fits(expected: &Type, actual: &Type, expr: &Expr) -> bool {
    if expected == actual || expected == &Type::One || actual == &Type::One {
        return true;
    }
    // A value that never arrives constrains nothing.
    if actual == &Type::Bottom {
        return true;
    }
    is_integer_literal(expr) && is_numeric(expected) && is_numeric(actual)
}

fn is_integer_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) => true,
        Expr::UnOp { op: slc_syntax::ast::UnOp::Neg, body } => is_integer_literal(&body.kind),
        _ => false,
    }
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Pos(Base::I32 | Base::I64 | Base::U32 | Base::U64))
}

fn is_comparable(ty: &Type) -> bool {
    is_numeric(ty)
        || matches!(ty, Type::Pos(Base::Char) | Type::Pos(Base::Str) | Type::Pos(Base::Bool))
}

/// A declaration's continuation row is positional and invariant: the
/// continuation supplied for a row position must have exactly the declared
/// type, and no position may be added, dropped, or reordered. Each
/// continuation is consumed exactly once, so a row that differs in width or
/// order is a different linear behavior, not a compatible one.
fn check_continuation_row(
    name: &str,
    signature: &FunctionSignature,
    args: &[Node<Expr>],
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) {
    let row_width = signature.continuations.iter().filter(|is_cont| **is_cont).count();
    if row_width == 0 {
        return;
    }
    if args.len() > signature.params.len() {
        diags.push(Diagnostic {
            message: format!(
                "`{name}` declares {} parameters including a continuation row of {row_width}; \
                 the call supplies {} arguments",
                signature.params.len(),
                args.len()
            ),
            span: args[signature.params.len()].span,
        });
        return;
    }
    for (index, arg) in args.iter().enumerate() {
        if signature.continuations.get(index) != Some(&true) {
            continue;
        }
        let expected = &signature.params[index];
        let Some(actual) = check_expr(arg, enums, env, diags) else {
            continue;
        };
        // `Type::One` is this checker's "not determined" placeholder — an
        // unannotated `let` binding, for instance. A row mismatch is only
        // reported for an argument whose type is actually known.
        if !fits(expected, &actual, &arg.kind) {
            diags.push(Diagnostic {
                message: format!(
                    "continuation row mismatch: argument {} of `{name}` has type {actual}; \
                     the row declares {expected} at that position",
                    index + 1
                ),
                span: arg.span,
            });
        }
    }
}

/// Check a `let` initializer against its optional annotation and return the
/// type to bind. Both surface `let` forms — the expression form and the
/// bodyless form that scopes over the rest of a block — check identically.
fn check_let_binding(
    name: &str,
    ty: &Option<TypeExpr>,
    value: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Type {
    let actual = check_expr(value, enums, env, diags);
    let annotation = ty.as_ref().and_then(|ty| lower_type(ty).ok());
    if let (Some(annotation), Some(actual)) = (&annotation, actual.clone())
        && !fits(annotation, &actual, &value.kind)
    {
        diags.push(Diagnostic {
            message: format!(
                "`let {name}` is annotated as {annotation}; initializer has type {actual}"
            ),
            span: value.span,
        });
    }
    annotation.or(actual).unwrap_or(Type::One)
}

/// Bind an arm's components and check that its pattern is a shape of the type
/// the `select` consumes.
fn bind_select_arm(
    consumed: &Type,
    pattern: &slc_syntax::ast::Pattern,
    declarations: &Declarations,
    env: &mut Env,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use slc_syntax::ast::Pattern;
    // A name that is not a variant binds the whole value: a type with no
    // structure has one shape, whose single component is the value itself.
    if let Pattern::Ident(name) = pattern
        && declarations.variant(name).is_none()
    {
        env.define(name, consumed.clone());
        return;
    }
    if matches!(pattern, Pattern::Wildcard) {
        return;
    }
    let components: Vec<Type> = match (consumed, pattern) {
        // An enum variant: its payload types.
        (Type::Named(_), Pattern::Ident(_) | Pattern::Enum { .. }) => {
            let written = match pattern {
                Pattern::Ident(name) => name.clone(),
                Pattern::Enum { name, variant, .. } => {
                    if variant.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}::{variant}")
                    }
                }
                _ => unreachable!("matched above"),
            };
            match declarations.variant(&written) {
                Some((_, payload)) => payload.clone(),
                None => {
                    diags.push(Diagnostic {
                        message: format!("`{written}` is not a variant of {consumed}"),
                        span,
                    });
                    return;
                }
            }
        }
        // A struct: its field types.
        (Type::Named(name), Pattern::Struct { name: written, .. }) => {
            if written != name {
                diags.push(Diagnostic {
                    message: format!("`select {consumed}` arm cannot bind a `{written}`"),
                    span,
                });
                return;
            }
            declarations.fields(name).unwrap_or_default()
        }
        // A tensor: its components, flattened right-nested.
        (Type::Tensor(..), Pattern::Tuple(_)) => flatten_tensor(consumed),
        _ => {
            diags.push(Diagnostic {
                message: format!("a `select {consumed}` arm must cover a shape of {consumed}"),
                span,
            });
            return;
        }
    };

    let binders: Vec<&slc_syntax::ast::Pattern> = match pattern {
        Pattern::Enum { fields, .. } => fields.iter().collect(),
        Pattern::Struct { fields, .. } => fields.iter().map(|(_, p)| p).collect(),
        Pattern::Tuple(items) => items.iter().collect(),
        _ => Vec::new(),
    };
    if binders.len() != components.len() {
        diags.push(Diagnostic {
            message: format!(
                "this shape has {} component(s); the arm binds {}",
                components.len(),
                binders.len()
            ),
            span,
        });
    }
    for (binder, ty) in binders.iter().zip(components) {
        if let Pattern::Ident(name) = binder {
            env.define(name, ty);
        }
    }
}

/// The components of a right-nested tensor.
fn flatten_tensor(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Tensor(head, rest) => {
            let mut out = vec![(**head).clone()];
            out.extend(flatten_tensor(rest));
            out
        }
        other => vec![other.clone()],
    }
}

fn infer_expr(
    e: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    check_expr(e, enums, env, diags)
}

fn check_expr(
    e: &Node<Expr>,
    enums: &Declarations,
    env: &mut Env,
    diags: &mut Vec<Diagnostic>,
) -> Option<Type> {
    if let Some(ty) = literal_type(&e.kind) {
        return Some(ty);
    }
    match &e.kind {
        Expr::Ident(name) => {
            if let Some(ty) = env.lookup(name) {
                return Some(ty);
            }
            if enums.declarations.contains(name) {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` is a declaration name, not a value; write a variant or a \
                         literal of it"
                    ),
                    span: e.span,
                });
                return None;
            }
            let (declaration, payload) = enums.variant(name)?;
            if !payload.is_empty() {
                diags.push(Diagnostic {
                    message: format!(
                        "variant `{name}` carries {} payload value(s); it is a value only when \
                         applied to them",
                        payload.len()
                    ),
                    span: e.span,
                });
                return None;
            }
            Some(Type::Named(declaration.clone()))
        }
        Expr::Lambda { param, param_type, body, .. } => {
            env.push();
            let param_ty = param_type.as_ref().and_then(|ty| enums.resolve(ty));
            if let Some(ty) = param_ty.clone() {
                env.define(param, ty);
            }
            let result = check_expr(body, enums, env, diags);
            env.pop();
            // A lambda is a function value, and its result is what the body
            // produces. A body that ends in a cut produces nothing, and
            // `A → ⊥` is `-A`, so such a lambda simply *is* a consumer.
            Some(Type::arrow(param_ty.unwrap_or(Type::One), result.unwrap_or(Type::One)))
        }
        Expr::Call { callee, args } => {
            // A variant applied to its payload is a value, not a call.
            if let Expr::Ident(name) = &callee.kind
                && env.lookup(name).is_none()
                && let Some((declaration, payload)) = enums.variant(name)
            {
                let declaration = declaration.clone();
                let payload = payload.clone();
                if args.len() != payload.len() {
                    diags.push(Diagnostic {
                        message: format!(
                            "variant `{name}` carries {} payload value(s); the expression \
                             supplies {}",
                            payload.len(),
                            args.len()
                        ),
                        span: e.span,
                    });
                }
                for (arg, expected) in args.iter().zip(payload.iter()) {
                    if let Some(actual) = check_expr(arg, enums, env, diags)
                        && !fits(expected, &actual, &arg.kind)
                    {
                        diags.push(Diagnostic {
                            message: format!(
                                "payload of `{name}` has type {actual}; the variant declares \
                                 {expected}"
                            ),
                            span: arg.span,
                        });
                    }
                }
                return Some(Type::Named(declaration));
            }
            // A continuation is not applied: it is cut against a value. Only
            // an atomic consumer is certainly not a function — `A → B` is
            // `-A ⅋ B`, so a function is negative too, and a `⅋` may be
            // either a function or a consumer of a product.
            if let Expr::Ident(name) = &callee.kind
                && let Some(ty) = env.lookup(name)
                && matches!(ty, Type::Neg(_) | Type::Bottom | Type::Dual(_))
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` is a consumer of type {ty}, not a function; send it a value \
                         with a cut: `value @ {name}`"
                    ),
                    span: e.span,
                });
                for arg in args {
                    check_expr(arg, enums, env, diags);
                }
                return Some(Type::Bottom);
            }
            check_expr(callee, enums, env, diags);
            for arg in args {
                check_expr(arg, enums, env, diags);
            }
            if let Expr::Ident(name) = &callee.kind
                && let Some(signature) = env.functions.get(name)
            {
                if is_builtin(name) {
                    for ((arg, param), is_continuation) in
                        args.iter().zip(signature.params.iter()).zip(&signature.continuations)
                    {
                        if *is_continuation {
                            continue;
                        }
                        if let Some(actual) = check_expr(arg, enums, env, diags)
                            && !fits(param, &actual, &arg.kind)
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
                check_continuation_row(name, signature, args, enums, env, diags);
                return signature.result.clone();
            }
            None
        }
        Expr::If { cond, then, otherwise } => {
            let cond_ty = check_expr(cond, enums, env, diags);
            if cond_ty != Some(Type::Pos(Base::Bool)) {
                diags.push(Diagnostic {
                    message: format!(
                        "`if` condition has type {}; expected +bool",
                        cond_ty.map(|ty| ty.to_string()).unwrap_or_else(|| "unknown".into())
                    ),
                    span: cond.span,
                });
            }
            let then_ty = check_expr(then, enums, env, diags);
            let Some(otherwise) = otherwise else {
                return then_ty;
            };
            let else_ty = check_expr(otherwise, enums, env, diags);
            // A branch that ends in a cut never returns, so it constrains
            // nothing: the `if` has the type of the branch that does return.
            match (then_ty, else_ty) {
                (Some(Type::Bottom), other) | (other, Some(Type::Bottom)) => other,
                (Some(then_ty), Some(else_ty)) => {
                    if then_ty != else_ty {
                        diags.push(Diagnostic {
                            message: format!(
                                "`if` branches have incompatible types {then_ty} and {else_ty}"
                            ),
                            span: otherwise.span,
                        });
                    }
                    Some(then_ty)
                }
                (then_ty, _) => then_ty,
            }
        }
        Expr::Let { name, ty, value, body } => {
            let binding_ty = check_let_binding(name, ty, value, enums, env, diags);
            env.push();
            env.define(name, binding_ty);
            let result = body.as_ref().and_then(|body| check_expr(body, enums, env, diags));
            env.pop();
            result
        }
        Expr::BinOp { op, lhs, rhs } => {
            let lhs_ty = check_expr(lhs, enums, env, diags);
            let rhs_ty = check_expr(rhs, enums, env, diags);
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
            let body_ty = check_expr(body, enums, env, diags);
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
            let value_ty = check_expr(value, enums, env, diags);
            let index_ty = check_expr(index, enums, env, diags);
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
            let value_ty = check_expr(value, enums, env, diags);
            for endpoint in [start, end].into_iter().flatten() {
                let endpoint_ty = check_expr(endpoint, enums, env, diags);
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
            let scrutinee_ty = check_expr(scrutinee, enums, env, diags);
            if let Some(scrutinee_ty) = &scrutinee_ty {
                for arm in arms {
                    check_pattern(&arm.pattern, scrutinee_ty, enums, arm.body.span, diags);
                }
            }
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    let guard_ty = check_expr(guard, enums, env, diags);
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
                check_expr(&arm.body, enums, env, diags);
            }
            None
        }
        Expr::Struct { name, fields } => {
            let Some(declared) = enums.structs.get(name).cloned() else {
                diags.push(Diagnostic {
                    message: format!("`{name}` is not a declared struct"),
                    span: e.span,
                });
                for (_, value) in fields {
                    check_expr(value, enums, env, diags);
                }
                return None;
            };
            // A struct literal is the product of its declared fields: every
            // field is present exactly once, in declaration order, with the
            // declared type.
            let written: Vec<&String> = fields.iter().map(|(field, _)| field).collect();
            let expected: Vec<&String> = declared.iter().map(|(field, _)| field).collect();
            if written != expected {
                diags.push(Diagnostic {
                    message: format!(
                        "`{name}` has fields {}; the literal writes {}",
                        expected.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", "),
                        if written.is_empty() {
                            "none".to_string()
                        } else {
                            written.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
                        }
                    ),
                    span: e.span,
                });
            }
            for (field, value) in fields {
                let actual = check_expr(value, enums, env, diags);
                if let (Some(actual), Some((_, expected))) =
                    (actual, declared.iter().find(|(declared, _)| declared == field))
                    && !fits(expected, &actual, &value.kind)
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "field `{field}` of `{name}` has type {actual}; the declaration \
                             says {expected}"
                        ),
                        span: value.span,
                    });
                }
            }
            Some(Type::Named(name.clone()))
        }
        Expr::Select { ty, arms } => {
            // `select T { p <= c, … }` builds the consumer of T. Each arm
            // covers one shape of T, binds that shape's components, and runs
            // a command; the whole expression is dual to T.
            let resolved = match ty {
                Some(ty) => match enums.resolve(&ty.kind) {
                    Some(resolved) => Some(resolved),
                    None => {
                        diags.push(Diagnostic {
                            message: "`select` needs a declared type or an explicit connective"
                                .into(),
                            span: ty.span,
                        });
                        return None;
                    }
                },
                // Left out: an arm's pattern may name the type, and inside a
                // negative `fn` the declaration already said it.
                None => named_by_arms(arms, enums).or_else(|| env.consumed.clone()),
            };
            let Some(resolved) = resolved else {
                diags.push(Diagnostic {
                    message: "no arm names a type, so write what this `select` consumes".into(),
                    span: e.span,
                });
                return None;
            };
            for arm in arms {
                env.push();
                bind_select_arm(&resolved, &arm.pattern, enums, env, e.span, diags);
                let command = check_expr(&arm.command, enums, env, diags);
                if let Some(command) = command
                    && command != Type::Bottom
                    && command != Type::One
                {
                    diags.push(Diagnostic {
                        message: format!(
                            "a `select` arm is a command; this one has type {command}"
                        ),
                        span: arm.command.span,
                    });
                }
                env.pop();
            }
            Some(resolved.dual())
        }
        Expr::Pair(items) if items.is_empty() => Some(Type::One),
        Expr::Pair(items) => items
            .iter()
            .map(|item| check_expr(item, enums, env, diags))
            .collect::<Option<Vec<_>>>()
            .map(|types| {
                types.into_iter().rev().reduce(|acc, ty| Type::Tensor(Box::new(ty), Box::new(acc)))
            })?,
        Expr::Block(exprs) => {
            let mut result = None;
            env.push();
            for expr in exprs {
                if let Expr::Let { name, ty, value, body: None } = &expr.kind {
                    // A bodyless `let` scopes over the rest of the block; the
                    // binding itself is checked exactly as the expression form.
                    let binding_ty = check_let_binding(name, ty, value, enums, env, diags);
                    env.define(name, binding_ty);
                } else {
                    result = check_expr(expr, enums, env, diags);
                }
            }
            env.pop();
            result
        }
        Expr::ErrorProp { expr, continuation } => {
            check_expr(expr, enums, env, diags);
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
        Expr::Cut { value, consumer } => {
            // `v @ k` is a command: it sends `v` to the consumer `k` and does
            // not return, so its type is bottom. A cut is well typed when the
            // two sides are dual — that is what makes the interaction fit,
            // and with `A → B` being `-A ⅋ B` it is the whole rule: which
            // side is written negatively is not itself the question.
            let value_ty = check_expr(value, enums, env, diags);
            let consumer_ty = check_expr(consumer, enums, env, diags);
            if let (Some(value_ty), Some(consumer_ty)) = (&value_ty, &consumer_ty)
                && value_ty != &Type::One
                && consumer_ty != &Type::One
                && !fits(&consumer_ty.dual(), value_ty, &value.kind)
            {
                diags.push(Diagnostic {
                    message: format!(
                        "`@` sends a value to a consumer of it: {consumer_ty} accepts {}, \
                         and the value has type {value_ty}",
                        consumer_ty.dual()
                    ),
                    span: e.span,
                });
            }
            Some(Type::Bottom)
        }
        Expr::Mu { value_params, continuation_params, body, .. } => {
            env.push();
            let mut captured_types = Vec::new();
            for p in value_params.iter().chain(continuation_params.iter()) {
                let ty = match &p.ty {
                    Some(ty) => enums.resolve(ty),
                    // Nothing was written, so the body says it.
                    None => infer_param_type(&p.name, body, enums, env),
                };
                if p.is_continuation || value_params.iter().all(|v| v.name != p.name) {
                    // Recorded in the order the row is written.
                }
                if let Some(ty) = ty.clone() {
                    env.define(&p.name, ty);
                }
                if continuation_params.iter().any(|c| c.name == p.name) {
                    captured_types.push(ty);
                }
            }
            let result = check_expr(body, enums, env, diags);
            env.pop();
            // A local `mu` captures the ambient continuation, so its value is
            // whatever that continuation receives: `mu f | (k: -A) { … }`
            // has type `A`. With value parameters, or with a row whose
            // positions disagree, there is no single such type.
            let captured = captured_types
                .into_iter()
                .map(|ty| ty.map(|ty| ty.dual()))
                .collect::<Option<Vec<_>>>()
                .filter(|types| {
                    value_params.is_empty()
                        && !types.is_empty()
                        && types.windows(2).all(|pair| pair[0] == pair[1])
                })
                .map(|mut types| types.remove(0));
            captured.or(result)
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
    fn struct_literal_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "struct Direction { left: i64, right: i64 }
                 fn f() -> i64 { use_it(Direction { left: 1, right: 2 }) }"
            )
            .is_ok()
        );

        let missing = check(
            "struct Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            missing.iter().any(|d| d.message.contains("the literal writes `left`")),
            "{missing:?}"
        );

        let reordered = check(
            "struct Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { right: 2, left: 1 }) }",
        )
        .unwrap_err();
        assert!(
            reordered.iter().any(|d| d.message.contains("the literal writes `right`, `left`")),
            "{reordered:?}"
        );
    }

    #[test]
    fn struct_pattern_must_write_every_declared_field_in_order() {
        assert!(
            check(
                "struct D { left: i64, right: i64 }
                 fn f(d: D) -> i64 {
                     match d {
                         D { left: a, right: b } => a,
                         _ => 0,
                     }
                 }"
            )
            .is_ok()
        );

        let diags = check(
            "struct D { left: i64, right: i64 }
             fn f(d: D) -> i64 {
                 match d {
                     D { right: b, left: a } => a,
                     _ => 0,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("the pattern writes `right`, `left`")),
            "{diags:?}"
        );
    }

    #[test]
    fn struct_pattern_field_types_are_checked() {
        let diags = check(
            "struct D { left: i64, right: i64 }
             fn f(d: D) -> i64 {
                 match d {
                     D { left: 1, right: 'c' } => 0,
                     _ => 0,
                 }
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("pattern has type +char")), "{diags:?}");
    }

    #[test]
    fn struct_pattern_cannot_match_another_type() {
        let diags = check(
            "struct D { left: i64 }
             fn f(x: +i64) -> i64 {
                 match x {
                     D { left: a } => a,
                     _ => 0,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("cannot match a scrutinee of type +i64")),
            "{diags:?}"
        );
    }

    #[test]
    fn struct_literal_field_types_are_checked() {
        let diags = check(
            "struct Direction { left: i64, right: i64 }
             fn f() -> i64 { use_it(Direction { left: 1, right: \"two\" }) }",
        )
        .unwrap_err();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("field `right` of `Direction` has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn undeclared_struct_literal_is_rejected() {
        let diags = check("fn f() -> i64 { use_it(Nope { a: 1 }) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`Nope` is not a declared struct")),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_variant_without_payload_is_a_value() {
        assert!(check("enum Color { Red } fn f() -> Color { Color::Red }").is_ok());
        assert!(check("enum Color { Red } fn f() -> Color { Red }").is_ok());
    }

    #[test]
    fn enum_variant_with_payload_is_a_value_only_when_applied() {
        // An integer literal has type `+i64`.
        assert!(check("enum R { Some(i64) } fn f() -> R { R::Some(1) }").is_ok());
        let diags = check("enum R { Some(i64) } fn f() -> R { R::Some }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("carries 1 payload value(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_variant_payload_arity_is_checked() {
        let diags = check("enum R { Both(i64, i64) } fn f() -> R { R::Both(1) }").unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("the expression supplies 1")), "{diags:?}");
    }

    #[test]
    fn enum_variant_payload_type_is_checked() {
        let diags = check("enum R { Some(i64) } fn f() -> R { R::Some(\"text\") }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("payload of `R::Some` has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn declaration_name_is_not_a_value() {
        for source in [
            "enum Color { Red } fn f() -> Color { Color }",
            "struct S { a: i32 } fn f() -> i32 { S }",
        ] {
            let diags = check(source).unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains("is a declaration name, not a value")),
                "{source}: {diags:?}"
            );
        }
    }

    #[test]
    fn select_arm_binds_the_variant_payload() {
        assert!(
            check(
                "enum R { Some(i64), None }
                 fn k(ok: -i64, absent: -i64) <- R {
                     select R {
                         Some(value) <= value @ ok,
                         None <= 0 @ absent,
                     }
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn select_arm_must_bind_a_declared_payload() {
        let diags = check(
            "enum R { Some(i64), None }
             fn k(ok: -i64, absent: -i64) <- R {
                 select R {
                     Some <= 0 @ ok,
                     None <= 0 @ absent,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("this shape has 1 component(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn select_arm_cannot_bind_a_payload_a_variant_does_not_have() {
        let diags = check(
            "enum R { None }
             fn k(absent: -i32) <- R {
                 select R {
                     None(value) <= 0 @ absent,
                 }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("this shape has 0 component(s)")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_continuation_is_cut_against_not_called() {
        let diags = check("command route(x: +i32) | (k: -i32) { k(x) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("is a consumer of type -i32, not a function")
                && d.message.contains("value @ k")),
            "{diags:?}"
        );
        assert!(check("command route(x: +i32) | (k: -i32) { x @ k }").is_ok());
    }

    #[test]
    fn exit_is_a_continuation() {
        let diags = check("fn main() -> i32 { EXIT(0) }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("`EXIT` is a consumer of type -i32")),
            "{diags:?}"
        );
        assert!(check("fn main() -> i32 { 0 @ EXIT }").is_ok());
    }

    #[test]
    fn a_cut_is_a_command() {
        // A cut has type ⊥: it produces nothing and control does not return,
        // so a branch that ends in one leaves the `if` type to the other.
        let ok = check(
            "fn parse(input: +String, err: -String) -> i64 {
                 if str_len(input) > 0 { 1 } else { \"empty\" @ err }
             }",
        );
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn a_cut_needs_dual_sides() {
        // Two values of the same positive type do not interact.
        let diags = check("fn f(x: +i32, y: +i32) -> i32 { x @ y }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("sends a value to a consumer of it")),
            "{diags:?}"
        );
    }

    #[test]
    fn a_cut_checks_what_the_consumer_accepts() {
        let diags = check("command route(x: +String) | (k: -i32) { x @ k }").unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("-i32 accepts +i32")
                && d.message.contains("the value has type +String")),
            "{diags:?}"
        );
    }

    #[test]
    fn applying_a_negative_function_to_a_continuation_is_application() {
        // Supplying a continuation to a negative function is an ordinary
        // call: the declared row says the argument is a consumer.
        assert!(
            check(
                "enum Color { Red, Green }
                 fn code(return: -i64) <- Color {
                     select Color {
                         Red <= 0 @ return,
                         Green <= 1 @ return,
                     }
                 }
                 fn main() -> i64 {
                     mu ask | (answer: -i64) { Color::Green @ code(answer) }
                 }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_accepts_the_declared_row() {
        assert!(
            check(
                "command route(x: +i32) | (k: -i32) { x @ k }
                 fn main() -> i32 { mu run | (out: -i32) { route(1, out) } }"
            )
            .is_ok()
        );
    }

    #[test]
    fn continuation_row_rejects_an_incompatible_continuation_type() {
        let diags = check(
            "command route(x: +i32) | (k: -i32) { x @ k }
             fn main() -> i32 { mu run | (out: -bool) { route(1, out) } }",
        )
        .unwrap_err();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("continuation row mismatch")
                    && d.message.contains("-bool")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn continuation_row_is_positional() {
        // The row is ordered: swapping two continuations of different types
        // is rejected even though both types appear in the declaration.
        let diags = check(
            "command route(a: -i32, b: -bool) | (c: -i32, d: -bool) { 0 @ c }
             fn main() -> i32 {
                 mu run | (first: -i32, second: -bool) { route(0, true, second, first) }
             }",
        )
        .unwrap_err();
        assert!(
            diags.iter().filter(|d| d.message.contains("continuation row mismatch")).count() == 2,
            "both swapped positions should be rejected: {diags:?}"
        );
    }

    #[test]
    fn continuation_row_rejects_extra_arguments() {
        let diags = check(
            "command route(x: +i32) | (k: -i32) { x @ k }
             fn main() -> i32 { mu run | (out: -i32) { route(1, out, out) } }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("continuation row of 1")),
            "diags: {diags:?}"
        );
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

    #[test]
    fn select_expression_has_dual_named_type() {
        let r = check(
            "enum Color { Red, Green, Blue }
            fn k(return: -i32) <- Color {
                select Color {
                    Red <= 0 @ return,
                    Green <= 1 @ return,
                    Blue <= 2 @ return,
                }
            }",
        );
        assert!(r.is_ok(), "unexpected diagnostics: {r:?}");
    }

    #[test]
    fn select_arm_must_be_a_command() {
        let diags = check(
            "enum Color { Red, Green }
            fn k(return: -i32) <- Color {
                select Color {
                    Red <= 0,
                    Green <= 1 @ return,
                }
            }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("`select` arm is a command")), "{diags:?}");
    }

    #[test]
    fn select_arm_must_cover_a_shape_of_the_type() {
        let diags = check(
            "enum Color { Red, Green }
            fn k(return: -i32) <- Color {
                select Color {
                    (a, b) <= 0 @ return,
                    Green <= 1 @ return,
                }
            }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("must cover a shape of")), "{diags:?}");
    }

    #[test]
    fn a_select_reads_its_type_off_its_arms() {
        // A bare variant name says which enum, so writing it again is
        // redundant.
        assert!(
            check(
                "enum Color { Red, Green }
                 fn code(return: -i32) <- Color {
                     select { Red <= 0 @ return, Green <= 1 @ return }
                 }"
            )
            .is_ok()
        );

        // Exhaustiveness of an inferred type is checked in `exhaustive`.
    }

    #[test]
    fn a_select_in_a_negative_fn_takes_the_type_it_consumes() {
        // Nothing in `n <= …` names a type, but the declaration already did.
        assert!(check("fn twice(out: -i64) <- +i64 { select { n <= (n * 2) @ out } }").is_ok());
        let diags = check("fn twice(out: -String) <- +i64 { select { n <= str_len(n) @ out } }")
            .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );

        // Outside one, with no arm naming a type, it has to be written.
        let diags = check(
            "command main | (exit: -i32) {
                 let show = select { n <= println(n) };
                 42 @ show;
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("no arm names a type")), "{diags:?}");
    }

    #[test]
    fn a_local_mu_takes_its_parameter_type_from_the_body() {
        // `k` is handed to a slot `read_file` declares, so it is `-String`,
        // and the `mu` therefore produces a `+String`.
        let diags = check(
            "command main | (exit: -i32) {
                 let text = mu | (k) { read_file(\"in\", k, complain) };
                 println(text + 1);
                 0 @ exit
             }
             fn complain(m: +String) -> ⊥ { println(m); 1 @ EXIT }",
        )
        .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("+String and +i64")),
            "the inferred type should reach the use: {diags:?}"
        );

        // A cut says it just as well: `42 @ k` makes `k` a consumer of i64.
        let diags = check(
            "command main | (exit: -i32) {
                 let answer = mu | (k) { 42 @ k };
                 println(str_len(answer));
                 0 @ exit
             }",
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("+i64")), "{diags:?}");
    }

    #[test]
    fn a_select_arm_over_an_atom_binds_the_whole_value() {
        // An atom has one shape and one component, so its arm's pattern is a
        // plain binder, typed by the type being consumed.
        assert!(
            check("fn show(out: -String) <- +i64 { select +i64 { n <= int_to_str(n) @ out } }")
                .is_ok()
        );

        let diags =
            check("fn show(out: -String) <- +i64 { select +i64 { n <= str_len(n) @ out } }")
                .unwrap_err();
        assert!(
            diags.iter().any(|d| d.message.contains("has type +i64")),
            "the binder should carry the consumed type: {diags:?}"
        );
    }
}
