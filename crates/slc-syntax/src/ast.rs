//! Surface AST for Slant.

use crate::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Node<T> {
    pub kind: T,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Ident(String),
    Lambda {
        param: String,
        param_type: Option<TypeExpr>,
        return_type: Option<TypeExpr>,
        body: Box<Node<Expr>>,
    },
    Call {
        callee: Box<Node<Expr>>,
        args: Vec<Node<Expr>>,
    },
    Pair(Vec<Node<Expr>>),
    Match {
        scrutinee: Box<Node<Expr>>,
        arms: Vec<MatchArm>,
    },
    /// A struct literal: `Direction { left: 0, right: 1 }`.
    Struct {
        name: String,
        fields: Vec<(String, Node<Expr>)>,
    },
    /// `select T { pattern <= command, … }` — the consumer of a positive
    /// type, given by cases on it. An `enum` has one arm per variant; a
    /// product has exactly one, binding its components.
    /// The type may be omitted when an arm's pattern names it: `Red` names
    /// its enum, `S { … }` names its struct. A product or an atom has no
    /// such name, so it is written.
    Select {
        ty: Option<Box<Node<TypeExpr>>>,
        arms: Vec<SelectArm>,
    },
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Box<Node<Expr>>,
        body: Option<Box<Node<Expr>>>,
    },
    If {
        cond: Box<Node<Expr>>,
        then: Box<Node<Expr>>,
        otherwise: Option<Box<Node<Expr>>>,
    },
    BinOp {
        op: BinOp,
        lhs: Box<Node<Expr>>,
        rhs: Box<Node<Expr>>,
    },
    UnOp {
        op: UnOp,
        body: Box<Node<Expr>>,
    },
    /// `↓e` boxes a negative expression as data; `↑e` opens the box. Both
    /// are erased at lowering — the check is what they are for.
    Shift {
        down: bool,
        expr: Box<Node<Expr>>,
    },
    /// A cut: `v @ k` sends the value `v` to the consumer `k`.
    ///
    /// A cut is a command, not an application: it has no result and control
    /// does not return from it. Application is `Call`, at either polarity.
    Cut {
        value: Box<Node<Expr>>,
        consumer: Box<Node<Expr>>,
    },
    /// A local μ abstraction: `mu(k) { body }`, which captures the
    /// continuation the expression is cut against. It has no value
    /// parameters — abstracting over a value is what `fn` does — and the
    /// name is optional, since nothing refers to it, as is a parameter's
    /// type, when the body says what it is.
    Mu {
        name: Option<String>,
        continuation_params: Vec<Param>,
        body: Box<Node<Expr>>,
    },
    ErrorProp {
        expr: Box<Node<Expr>>,
        continuation: Option<String>,
    },
    /// `handle body with E { op(p) resume => b, …, return(x) => r }`: run
    /// `body`, answering each performed operation of effect `E` with its
    /// clause, and its normal result with the `return` clause.
    Handle {
        body: Box<Node<Expr>>,
        effect: String,
        clauses: Vec<HandleClause>,
        /// The `return(x) => r` clause: its binder and body.
        ret: Option<(String, Box<Node<Expr>>)>,
    },
    /// A sequence of expressions; the value of the last one.
    Block(Vec<Node<Expr>>),
    Index {
        value: Box<Node<Expr>>,
        index: Box<Node<Expr>>,
    },
    Slice {
        value: Box<Node<Expr>>,
        start: Option<Box<Node<Expr>>>,
        end: Option<Box<Node<Expr>>>,
    },
}

impl Expr {
    /// Every immediate sub-expression, in source order. A walk over an
    /// expression needs no case for each node this way.
    pub fn children(&self) -> Vec<&Node<Expr>> {
        match self {
            Expr::Int(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Char(_)
            | Expr::Bool(_)
            | Expr::Ident(_) => Vec::new(),
            Expr::Lambda { body, .. } | Expr::UnOp { body, .. } | Expr::Mu { body, .. } => {
                vec![body]
            }
            Expr::Call { callee, args } => std::iter::once(&**callee).chain(args).collect(),
            Expr::Pair(items) | Expr::Block(items) => items.iter().collect(),
            Expr::Match { scrutinee, arms } => std::iter::once(&**scrutinee)
                .chain(arms.iter().flat_map(|a| a.guard.iter().chain(std::iter::once(&a.body))))
                .collect(),
            Expr::Struct { fields, .. } => fields.iter().map(|(_, value)| value).collect(),
            Expr::Select { arms, .. } => arms.iter().map(|arm| &arm.command).collect(),
            Expr::Let { value, body, .. } => {
                std::iter::once(&**value).chain(body.iter().map(|b| &**b)).collect()
            }
            Expr::If { cond, then, otherwise } => std::iter::once(&**cond)
                .chain(std::iter::once(&**then))
                .chain(otherwise.iter().map(|e| &**e))
                .collect(),
            Expr::BinOp { lhs, rhs, .. } => vec![lhs, rhs],
            Expr::Cut { value, consumer } => vec![value, consumer],
            Expr::ErrorProp { expr, .. } | Expr::Shift { expr, .. } => vec![expr],
            Expr::Handle { body, clauses, ret, .. } => std::iter::once(&**body)
                .chain(clauses.iter().map(|c| &c.body))
                .chain(ret.iter().map(|(_, b)| &**b))
                .collect(),
            Expr::Index { value, index } => vec![value, index],
            Expr::Slice { value, start, end } => std::iter::once(&**value)
                .chain(start.iter().map(|e| &**e))
                .chain(end.iter().map(|e| &**e))
                .collect(),
        }
    }
}

/// One operation clause of a handler: `op(params) resume => body`.
#[derive(Debug, Clone, PartialEq)]
pub struct HandleClause {
    pub op: String,
    pub params: Vec<String>,
    pub resume: String,
    pub body: Node<Expr>,
}

/// The polarity of a function declaration.
///
/// A positive function consumes values and produces a value. A negative
/// function consumes continuations and produces a continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionPolarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Base(String),
    Positive(Box<Node<TypeExpr>>),
    Negative(Box<Node<TypeExpr>>),
    Tensor(Box<Node<TypeExpr>>, Box<Node<TypeExpr>>),
    Par(Box<Node<TypeExpr>>, Box<Node<TypeExpr>>),
    Fun(Box<Node<TypeExpr>>, Box<Node<TypeExpr>>),
    List(Box<Node<TypeExpr>>),
    Dual(Box<Node<TypeExpr>>),
    /// `↓A` — a negative type boxed as data, and `↑A` — the computation that
    /// returns a positive one. Duals of each other, and neither is the
    /// identity: they are what stops `¬¬A` collapsing to `A`.
    Down(Box<Node<TypeExpr>>),
    Up(Box<Node<TypeExpr>>),
    Unit,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    /// `None` where the type was left out. A declaration's parameters always
    /// carry one — a declaration is an interface — so this is `None` only for
    /// the parameters of a local `mu`.
    pub ty: Option<TypeExpr>,
    pub is_continuation: bool,
}

/// One arm of a `select`, written `pattern <= command`: the shape that
/// selects it, and the command that runs when it arrives. The pattern's
/// binders scope over the command.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectArm {
    pub pattern: Pattern,
    pub command: Node<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Node<Expr>>,
    pub body: Node<Expr>,
}

/// What a pattern names: a declaration outright, or a variant whose
/// declaration the caller looks up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named<'a> {
    Declaration(&'a str),
    Variant(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Ident(String),
    Int(i64),
    Str(String),
    Char(char),
    Bool(bool),
    Float(f64),
    Or(Vec<Pattern>),
    Range { start: Box<Pattern>, end: Box<Pattern> },
    Binding { name: String, pattern: Box<Pattern> },
    Rest,
    Tuple(Vec<Pattern>),
    List { items: Vec<Pattern>, rest: Option<Box<Pattern>> },
    Struct { name: String, fields: Vec<(String, Pattern)> },
    Enum { name: String, variant: String, fields: Vec<Pattern> },
}

impl Pattern {
    /// The type this pattern names, when it names one. A `select` whose type
    /// is left out reads it off its arms: `S { … }` names its struct,
    /// `Color::Red(x)` its enum, and a bare `Red` its variant, whose
    /// declaration the caller resolves. A tuple or a plain binder names
    /// nothing — a product and an atom have no name of their own.
    pub fn names(&self) -> Option<Named<'_>> {
        match self {
            Pattern::Struct { name, .. } => Some(Named::Declaration(name)),
            Pattern::Enum { name, variant, .. } if variant.is_empty() => Some(Named::Variant(name)),
            Pattern::Enum { name, .. } => Some(Named::Declaration(name)),
            Pattern::Ident(name) => Some(Named::Variant(name)),
            Pattern::Binding { pattern, .. } => pattern.names(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Struct {
        name: String,
        fields: Vec<(String, TypeExpr)>,
    },
    Enum {
        name: String,
        variants: Vec<(String, Vec<TypeExpr>)>,
    },
    Fn {
        name: String,
        type_params: Vec<String>,
        /// Trait bounds on the type parameters: `(T, Show)` for `<T: Show>`.
        bounds: Vec<(String, String)>,
        polarity: FunctionPolarity,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Node<Expr>,
    },
    /// A declaration whose body is a command: it takes values and
    /// continuations and never returns. `mu` is the expression that captures
    /// the current continuation; this abstracts over one instead.
    Command {
        name: String,
        type_params: Vec<String>,
        bounds: Vec<(String, String)>,
        value_params: Vec<Param>,
        continuation_params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Node<Expr>,
    },
    /// A module: a named scope of declarations. Resolution flattens it,
    /// qualifying each declaration as `module::name`.
    Mod {
        name: String,
        decls: Vec<Node<Decl>>,
    },
    /// `use a::b::name;` — brings `name` into scope for the enclosing
    /// module.
    Use {
        path: Vec<String>,
    },
    Const {
        name: String,
        ty: TypeExpr,
        value: Node<Expr>,
    },
    /// A trait: a named set of method signatures over an implicit `Self`.
    Trait {
        name: String,
        methods: Vec<TraitMethod>,
    },
    /// An `impl Trait for Type { … }`: the methods that make `Type` satisfy
    /// `Trait`. Each method is a `Fn` or `Command` declaration with a body.
    Impl {
        trait_name: String,
        /// `impl<T: Show>` type parameters and bounds, shared by the methods.
        type_params: Vec<String>,
        bounds: Vec<(String, String)>,
        for_type: TypeExpr,
        methods: Vec<Node<Decl>>,
    },
    /// An effect: a named set of operations a computation may perform.
    Effect {
        name: String,
        operations: Vec<EffectOp>,
    },
}

/// One operation of an effect: a value-returning signature. `choose() -> bool`.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectOp {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
}

/// One method signature in a trait, headed like a `fn` or a `command` but
/// ending in `;` instead of a body. `Self` stands for the implementing type.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    /// `true` for a `command` method (value and continuation groups); `false`
    /// for a `fn` method (one group, a return type).
    pub is_command: bool,
    pub polarity: FunctionPolarity,
    pub value_params: Vec<Param>,
    pub continuation_params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub decls: Vec<Node<Decl>>,
}
