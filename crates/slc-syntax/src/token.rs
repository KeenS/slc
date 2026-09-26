//! Tokens with source spans.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    // Identifiers and keywords
    Ident(String),
    /// The lambda, `fn(x) { … }` and `fn { … }`. A named function is `Func`.
    Fn,
    /// A named function, `func`.
    Func,
    Mu,
    /// A `proc`.
    Command,
    Mod,
    Use,
    /// A `spec`.
    Trait,
    Impl,
    For,
    /// A `hook`.
    Effect,
    /// Inline handling, `do`.
    Handle,
    /// A handler value, `op`, and the `op h do e` form.
    Handler,
    Reset,
    /// Scrutinee matching, `of`.
    Match,
    Let,
    Data,
    Enum,
    Menu,
    Form,
    Dual,
    /// A definition, `def`.
    Const,
    Pub,
    /// A keyword the surface no longer spells this way. `replacement` is the
    /// spelling the parser names in the error.
    Retired {
        found: &'static str,
        replacement: &'static str,
    },

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
    Amp,
    AmpAmp,
    PipePipe,
    Bang,

    // Unicode
    CutOpen,  // ⟨
    CutClose, // ⟩
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
            TokenKind::Ident(_) => write!(f, "identifier"),
            TokenKind::Fn => write!(f, "`fn`"),
            TokenKind::Func => write!(f, "`func`"),
            TokenKind::Mu => write!(f, "`mu`"),
            TokenKind::Command => write!(f, "`proc`"),
            TokenKind::Mod => write!(f, "`mod`"),
            TokenKind::Use => write!(f, "`use`"),
            TokenKind::Trait => write!(f, "`spec`"),
            TokenKind::Impl => write!(f, "`impl`"),
            TokenKind::For => write!(f, "`for`"),
            TokenKind::Effect => write!(f, "`hook`"),
            TokenKind::Handle => write!(f, "`do`"),
            TokenKind::Handler => write!(f, "`op`"),
            TokenKind::Reset => write!(f, "`reset`"),
            TokenKind::Match => write!(f, "`of`"),
            TokenKind::Let => write!(f, "`let`"),
            TokenKind::Data => write!(f, "`data`"),
            TokenKind::Enum => write!(f, "`enum`"),
            TokenKind::Menu => write!(f, "`menu`"),
            TokenKind::Form => write!(f, "`form`"),
            TokenKind::Dual => write!(f, "`dual`"),
            TokenKind::Const => write!(f, "`def`"),
            TokenKind::Pub => write!(f, "`pub`"),
            TokenKind::Retired { found, .. } => write!(f, "`{found}`"),
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
            TokenKind::Amp => write!(f, "`&`"),
            TokenKind::CutOpen => write!(f, "`⟨`"),
            TokenKind::CutClose => write!(f, "`⟩`"),
            TokenKind::AmpAmp => write!(f, "`&&`"),
            TokenKind::PipePipe => write!(f, "`||`"),
            TokenKind::Bang => write!(f, "`!`"),
        }
    }
}
