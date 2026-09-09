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
    Mu {
        binder: Option<(String, Option<TypeExpr>)>,
        return_type: Option<TypeExpr>,
        body: Box<Node<Expr>>,
    },
    CommandDef {
        name: String,
        params: Vec<Param>,
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
    Interaction {
        left: Box<Node<Expr>>,
        right: Box<Node<Expr>>,
    },
    Spawn {
        body: Box<Node<Expr>>,
    },
    Dual {
        body: Box<Node<Expr>>,
    },
    ErrorProp {
        expr: Box<Node<Expr>>,
    },
    /// `agent.to(k, h)` — wire continuation ports first.
    Service {
        agent: Box<Node<Expr>>,
        continuations: Vec<Node<Expr>>,
    },
    /// `fn.partial(a)` — wire value ports first.
    Job {
        agent: Box<Node<Expr>>,
        values: Vec<Node<Expr>>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum PartialKind {
    Service,
    Job,
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
    /// `Command<I, O>` — the symmetric agent type.
    Command(Box<Node<TypeExpr>>, Box<Node<TypeExpr>>),
    Unit,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub is_continuation: bool,
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
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Node<Expr>,
    },
    Command {
        name: String,
        params: Vec<Param>,
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
