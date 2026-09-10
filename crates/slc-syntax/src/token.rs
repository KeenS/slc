//! Tokens with source spans.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),

    // Identifiers and keywords
    Ident(String),
    Fn,
    Mu,
    Command,
    Mod,
    Use,
    Trait,
    Impl,
    For,
    Effect,
    Handle,
    Match,
    Select,
    Let,
    If,
    Else,
    Struct,
    Enum,
    Dual,
    Return,
    Const,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Colon,
    ColonColon,
    Dot,
    Arrow,
    ReverseArrow,
    FatArrow,
    At,
    Question,
    DotDot,
    DotDotEq,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    Assign,
    Pipe,
    AmpAmp,
    PipePipe,
    Bang,

    // Unicode
    Tensor, // ⊗
    Par,    // ⅋
    Down,   // ↓
    Up,     // ↑
    Bot,    // ⊥
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Int(_) => write!(f, "integer"),
            TokenKind::Float(_) => write!(f, "float"),
            TokenKind::Str(_) => write!(f, "string"),
            TokenKind::Char(_) => write!(f, "char"),
            TokenKind::Bool(_) => write!(f, "bool"),
            TokenKind::Ident(_) => write!(f, "identifier"),
            TokenKind::Fn => write!(f, "`fn`"),
            TokenKind::Mu => write!(f, "`mu`"),
            TokenKind::Command => write!(f, "`command`"),
            TokenKind::Mod => write!(f, "`mod`"),
            TokenKind::Use => write!(f, "`use`"),
            TokenKind::Trait => write!(f, "`trait`"),
            TokenKind::Impl => write!(f, "`impl`"),
            TokenKind::For => write!(f, "`for`"),
            TokenKind::Effect => write!(f, "`effect`"),
            TokenKind::Handle => write!(f, "`handle`"),
            TokenKind::Match => write!(f, "`match`"),
            TokenKind::Select => write!(f, "`select`"),
            TokenKind::Let => write!(f, "`let`"),
            TokenKind::If => write!(f, "`if`"),
            TokenKind::Else => write!(f, "`else`"),
            TokenKind::Struct => write!(f, "`struct`"),
            TokenKind::Enum => write!(f, "`enum`"),
            TokenKind::Dual => write!(f, "`dual`"),
            TokenKind::Return => write!(f, "`return`"),
            TokenKind::Const => write!(f, "`const`"),
            TokenKind::LParen => write!(f, "`(`"),
            TokenKind::RParen => write!(f, "`)`"),
            TokenKind::LBrace => write!(f, "`{{`"),
            TokenKind::RBrace => write!(f, "`}}`"),
            TokenKind::LBracket => write!(f, "`[`"),
            TokenKind::RBracket => write!(f, "`]`"),
            TokenKind::Comma => write!(f, "`,`"),
            TokenKind::Semicolon => write!(f, "`;`"),
            TokenKind::Colon => write!(f, "`:`"),
            TokenKind::ColonColon => write!(f, "`::`"),
            TokenKind::Dot => write!(f, "`.`"),
            TokenKind::Arrow => write!(f, "`->`"),
            TokenKind::ReverseArrow => write!(f, "`<-`"),
            TokenKind::FatArrow => write!(f, "`=>`"),
            TokenKind::At => write!(f, "`@`"),
            TokenKind::Question => write!(f, "`?`"),
            TokenKind::DotDot => write!(f, "`..`"),
            TokenKind::DotDotEq => write!(f, "`..=`"),
            TokenKind::Plus => write!(f, "`+`"),
            TokenKind::Minus => write!(f, "`-`"),
            TokenKind::Star => write!(f, "`*`"),
            TokenKind::Slash => write!(f, "`/`"),
            TokenKind::Percent => write!(f, "`%`"),
            TokenKind::EqEq => write!(f, "`==`"),
            TokenKind::NotEq => write!(f, "`!=`"),
            TokenKind::Lt => write!(f, "`<`"),
            TokenKind::Gt => write!(f, "`>`"),
            TokenKind::Le => write!(f, "`<=`"),
            TokenKind::Ge => write!(f, "`>=`"),
            TokenKind::Assign => write!(f, "`=`"),
            TokenKind::Pipe => write!(f, "`|`"),
            TokenKind::AmpAmp => write!(f, "`&&`"),
            TokenKind::PipePipe => write!(f, "`||`"),
            TokenKind::Bang => write!(f, "`!`"),
            TokenKind::Tensor => write!(f, "`⊗`"),
            TokenKind::Down => write!(f, "`↓`"),
            TokenKind::Up => write!(f, "`↑`"),
            TokenKind::Par => write!(f, "`⅋`"),
            TokenKind::Bot => write!(f, "`⊥`"),
        }
    }
}
