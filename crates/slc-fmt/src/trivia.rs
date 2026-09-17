//! The token stream with its comments kept.
//!
//! The lexer drops comments and whitespace, but it leaves spans, and all
//! that lies between two tokens is whitespace and comments. So the gaps are
//! read again here: each token carries the comments written before it, and
//! each of those how many newlines came before — which is all the layout a
//! formatter keeps: whether a comment trails a line or has its own, and
//! where a blank line was.

use slc_syntax::token::{Span, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct Comment {
    pub text: String,
    /// Newlines between what came before — a token or a comment — and this.
    pub newlines_before: usize,
    /// `// …`, which ends its line; a `/* … */` does not.
    pub is_line: bool,
}

#[derive(Debug, Clone)]
pub struct Tok {
    pub kind: TokenKind,
    /// The token as written: `1_000` stays `1_000`, and a string keeps its
    /// escapes.
    pub text: String,
    pub span: Span,
    pub comments: Vec<Comment>,
    /// Newlines between the last comment before it — or the previous token —
    /// and this token.
    pub newlines_before: usize,
    /// Nothing at all between the previous token and this one.
    pub touches_previous: bool,
}

pub struct Stream {
    pub toks: Vec<Tok>,
    /// Comments after the last token.
    pub trailing: Vec<Comment>,
}

/// The comments in a gap, and the newlines after the last of them.
fn read_gap(gap: &[char]) -> (Vec<Comment>, usize) {
    let mut comments = Vec::new();
    let mut newlines = 0;
    let mut i = 0;
    while i < gap.len() {
        let is_line = gap[i] == '/' && gap.get(i + 1) == Some(&'/');
        let is_block = gap[i] == '/' && gap.get(i + 1) == Some(&'*');
        if !is_line && !is_block {
            newlines += usize::from(gap[i] == '\n');
            i += 1;
            continue;
        }
        let start = i;
        if is_line {
            while i < gap.len() && gap[i] != '\n' {
                i += 1;
            }
        } else {
            let mut depth = 0usize;
            loop {
                if gap[i..].starts_with(&['/', '*']) {
                    depth += 1;
                    i += 2;
                } else if gap[i..].starts_with(&['*', '/']) {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
                if i >= gap.len() {
                    break;
                }
            }
        }
        let text: String = gap[start..i.min(gap.len())].iter().collect();
        comments.push(Comment {
            text: text.trim_end().to_string(),
            newlines_before: newlines,
            is_line,
        });
        newlines = 0;
    }
    (comments, newlines)
}

/// `<-1 | k>` lexes its opening as `<-`. A number touching it is the
/// chain's first value, so the token is the chain's `<` and the number's
/// `-`, as the parser reads it. No type holds a number, so nowhere else
/// does `<-` touch one.
pub fn split_arrows(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut iter = tokens.into_iter().peekable();
    while let Some(token) = iter.next() {
        let touching_number = token.kind == TokenKind::ReverseArrow
            && iter.peek().is_some_and(|next| {
                next.span.start == token.span.end
                    && matches!(next.kind, TokenKind::Int(_) | TokenKind::Float(_))
            });
        if touching_number {
            let Span { start, end } = token.span;
            out.push(Token { kind: TokenKind::Lt, span: Span { start, end: start + 1 } });
            out.push(Token { kind: TokenKind::Minus, span: Span { start: start + 1, end } });
        } else {
            out.push(token);
        }
    }
    out
}

impl Stream {
    pub fn new(source: &str, tokens: Vec<Token>) -> Stream {
        let chars: Vec<char> = source.chars().collect();
        let mut toks = Vec::new();
        let mut at = 0;
        for token in split_arrows(tokens) {
            let (comments, newlines_before) = read_gap(&chars[at..token.span.start]);
            toks.push(Tok {
                text: chars[token.span.start..token.span.end].iter().collect(),
                touches_previous: !toks.is_empty() && at == token.span.start,
                kind: token.kind,
                span: token.span,
                comments,
                newlines_before,
            });
            at = token.span.end;
        }
        let (trailing, _) = read_gap(&chars[at..]);
        Stream { toks, trailing }
    }
}
