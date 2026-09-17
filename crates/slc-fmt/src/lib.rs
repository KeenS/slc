//! `slc fmt`: one layout for Slant source.
//!
//! Formatting decides whitespace, line breaks, and the separators the
//! grammar leaves optional. It never changes what a program means, and it
//! checks that it has not: the output must parse to the tree the input
//! parsed to, with every comment still in it, or nothing is returned.

mod doc;
mod format;
mod trivia;

use slc_syntax::token::Span;

/// The width a line is kept within, and the indentation of a level — the
/// project's `rustfmt.toml`, for the language it builds.
pub const MAX_WIDTH: usize = 100;
pub const INDENT: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub enum FormatError {
    /// The source does not lex or parse, so it has no layout to give.
    Syntax { message: String, span: Span },
    /// The formatter's own failure: it lost its place in the grammar, or
    /// its output does not mean what its input did. Nothing is written.
    Internal(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Syntax { message, .. } => write!(f, "{message}"),
            FormatError::Internal(message) => {
                write!(f, "internal formatter error (the file is untouched): {message}")
            }
        }
    }
}

/// What a source means to the parser, spans aside, and its comments in
/// order: the two things a layout must leave alone.
fn meaning(source: &str) -> Result<(String, Vec<String>), FormatError> {
    let tokens = slc_syntax::lexer::lex(source)
        .map_err(|e| FormatError::Syntax { message: e.message, span: e.span })?;
    let stream = trivia::Stream::new(source, tokens.clone());
    let comments = stream
        .toks
        .iter()
        .flat_map(|tok| &tok.comments)
        .chain(&stream.trailing)
        .map(|comment| comment.text.clone())
        .collect();
    let program = slc_syntax::parser::parse(tokens).map_err(|errors| {
        let first = &errors[0];
        FormatError::Syntax { message: format!("parse error: {}", first.message), span: first.span }
    })?;
    Ok((without_spans(&format!("{program:?}")), comments))
}

/// A tree's debug text with every `Span { … }` blanked: where a node was
/// is the one thing formatting is meant to change.
fn without_spans(debug: &str) -> String {
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(at) = rest.find("Span {") {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        rest = rest.find('}').map_or("", |end| &rest[end + 1..]);
    }
    out.push_str(rest);
    out
}

pub fn format_source(source: &str) -> Result<String, FormatError> {
    let before = meaning(source)?;
    let tokens = slc_syntax::lexer::lex(source)
        .map_err(|e| FormatError::Syntax { message: e.message, span: e.span })?;
    let mut formatter = format::Formatter::new(trivia::Stream::new(source, tokens));
    let doc = formatter.program().map_err(|e| FormatError::Internal(e.message))?;
    let formatted = doc::print(&doc, MAX_WIDTH, INDENT);
    match meaning(&formatted) {
        Ok(after) if after == before => Ok(formatted),
        Ok((_, comments)) if comments != before.1 => {
            Err(FormatError::Internal("the output lost or moved a comment".into()))
        }
        Ok(_) => Err(FormatError::Internal("the output parses to a different program".into())),
        Err(e) => Err(FormatError::Internal(format!("the output does not parse: {e}"))),
    }
}
