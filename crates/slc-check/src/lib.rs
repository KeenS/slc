//! The surface checkers: types, polarity, linearity, exhaustiveness, and
//! declaration inference, all reporting the same kind of diagnostic.

mod declarations;
mod env;
pub mod exhaustive;
pub mod expr;
pub mod inference;
pub mod linearity;
pub mod polarity;
mod signatures;

/// A checker finding: what went wrong, and the source bytes it is about.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: slc_syntax::token::Span,
}
