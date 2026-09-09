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
    /// `select T { command => pattern, … }` — the consumer of a positive
    /// type, given by cases on it. An `enum` has one arm per variant; a
    /// product has exactly one, binding its components.
    Select {
        ty: Box<Node<TypeExpr>>,
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
    /// A cut: `v @ k` sends the value `v` to the consumer `k`.
    ///
    /// A cut is a command, not an application: it has no result and control
    /// does not return from it. Application is `Call`, at either polarity.
    Cut {
        value: Box<Node<Expr>>,
        consumer: Box<Node<Expr>>,
    },
    /// A local μ abstraction: `mu name() | (k: -T) { body }`.
    Mu {
        name: String,
        value_params: Vec<Param>,
        continuation_params: Vec<Param>,
        body: Box<Node<Expr>>,
    },
    ErrorProp {
        expr: Box<Node<Expr>>,
        continuation: Option<String>,
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
    Unit,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub is_continuation: bool,
}

/// One arm of a `select`: the shape that selects it, and the command that
/// runs when it arrives. The pattern's binders scope over the command.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectArm {
    pub command: Node<Expr>,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Node<Expr>>,
    pub body: Node<Expr>,
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
        polarity: FunctionPolarity,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Node<Expr>,
    },
    Mu {
        name: String,
        value_params: Vec<Param>,
        continuation_params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Node<Expr>,
    },
    Const {
        name: String,
        ty: TypeExpr,
        value: Node<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub decls: Vec<Node<Decl>>,
}
