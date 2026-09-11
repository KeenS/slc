//! Lexer for Slant source files.

use crate::token::{Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Comments
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            let mut depth = 1;
            i += 2;
            while i < chars.len() && depth > 0 {
                if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                return Err(LexError {
                    message: "unterminated block comment".into(),
                    span: Span { start: i, end: i },
                });
            }
            continue;
        }

        let start = i;

        // Identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            let mut ident = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]);
                i += 1;
            }
            let kind = match ident.as_str() {
                "fn" => TokenKind::Fn,
                "mu" => TokenKind::Mu,
                "command" => TokenKind::Command,
                "mod" => TokenKind::Mod,
                "use" => TokenKind::Use,
                "trait" => TokenKind::Trait,
                "impl" => TokenKind::Impl,
                "for" => TokenKind::For,
                "effect" => TokenKind::Effect,
                "handle" => TokenKind::Handle,
                "match" => TokenKind::Match,
                "select" => TokenKind::Select,
                "let" => TokenKind::Let,
                "if" => TokenKind::If,
                "else" => TokenKind::Else,
                "data" => TokenKind::Data,
                "enum" => TokenKind::Enum,
                "menu" => TokenKind::Menu,
                "dual" => TokenKind::Dual,
                "return" => TokenKind::Return,
                "const" => TokenKind::Const,
                "true" => TokenKind::Bool(true),
                "false" => TokenKind::Bool(false),
                _ => TokenKind::Ident(ident),
            };
            tokens.push(Token { kind, span: Span { start, end: i } });
            continue;
        }

        // Numbers
        if c.is_ascii_digit() {
            let mut num = String::new();
            let mut is_float = false;
            while i < chars.len() {
                if chars[i] == '.' {
                    // `1..5` is a range, not a float.
                    if i + 1 < chars.len() && chars[i + 1] == '.' {
                        break;
                    }
                    if i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == '=' {
                        break;
                    }
                    is_float = true;
                } else if !(chars[i].is_ascii_digit() || chars[i] == '_') {
                    break;
                }
                if chars[i] != '_' {
                    num.push(chars[i]);
                }
                i += 1;
            }
            let kind = if is_float {
                TokenKind::Float(num.parse().unwrap())
            } else {
                TokenKind::Int(num.parse().unwrap())
            };
            tokens.push(Token { kind, span: Span { start, end: i } });
            continue;
        }

        // Strings
        if c == '"' {
            i += 1;
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    return Err(LexError {
                        message: "unterminated string".into(),
                        span: Span { start, end: i },
                    });
                }
                match chars[i] {
                    '"' => {
                        i += 1;
                        break;
                    }
                    '\\' => {
                        i += 1;
                        let (ch, next) = lex_escape(&chars, i, start)?;
                        s.push(ch);
                        i = next;
                    }
                    ch => {
                        s.push(ch);
                        i += 1;
                    }
                }
            }
            tokens.push(Token { kind: TokenKind::Str(s), span: Span { start, end: i } });
            continue;
        }

        // Chars
        if c == '\'' {
            i += 1;
            if i >= chars.len() {
                return Err(LexError {
                    message: "unterminated char literal".into(),
                    span: Span { start, end: i },
                });
            }
            let ch = if chars[i] == '\\' {
                i += 1;
                let (ch, next) = lex_escape(&chars, i, start)?;
                i = next;
                ch
            } else {
                let ch = chars[i];
                i += 1;
                ch
            };
            if i >= chars.len() || chars[i] != '\'' {
                return Err(LexError {
                    message: "unterminated char literal".into(),
                    span: Span { start, end: i },
                });
            }
            i += 1;
            tokens.push(Token { kind: TokenKind::Char(ch), span: Span { start, end: i } });
            continue;
        }

        // Operators and punctuation
        let two = if i + 1 < chars.len() { Some([chars[i], chars[i + 1]]) } else { None };
        if c == '.' && i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == '=' {
            i += 3;
            tokens.push(Token { kind: TokenKind::DotDotEq, span: Span { start, end: i } });
            continue;
        }
        if let Some([a, b]) = two {
            let kind = match (a, b) {
                ('=', '=') => Some(TokenKind::EqEq),
                ('!', '=') => Some(TokenKind::NotEq),
                ('<', '=') => Some(TokenKind::Le),
                ('>', '=') => Some(TokenKind::Ge),
                ('-', '>') => Some(TokenKind::Arrow),
                ('<', '-') => Some(TokenKind::ReverseArrow),
                ('=', '>') => Some(TokenKind::FatArrow),
                (':', ':') => Some(TokenKind::ColonColon),
                ('&', '&') => Some(TokenKind::AmpAmp),
                ('|', '|') => Some(TokenKind::PipePipe),
                ('.', '.') => Some(TokenKind::DotDot),
                _ => None,
            };
            if let Some(kind) = kind {
                i += 2;
                tokens.push(Token { kind, span: Span { start, end: i } });
                continue;
            }
        }

        let kind = match c {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,
            '@' => TokenKind::At,
            '!' => TokenKind::Bang,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '|' => TokenKind::Pipe,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            '=' => TokenKind::Assign,
            '⊗' => TokenKind::Tensor,
            '↓' => TokenKind::Down,
            '↑' => TokenKind::Up,
            '⅋' => TokenKind::Par,
            '⊥' => TokenKind::Bot,
            other => {
                return Err(LexError {
                    message: format!("unexpected character: {other}"),
                    span: Span { start, end: i + 1 },
                });
            }
        };
        i += 1;
        tokens.push(Token { kind, span: Span { start, end: i } });
    }

    Ok(tokens)
}

fn lex_escape(chars: &[char], i: usize, literal_start: usize) -> Result<(char, usize), LexError> {
    if i >= chars.len() {
        return Err(LexError {
            message: "unterminated escape".into(),
            span: Span { start: literal_start, end: i },
        });
    }
    let ch = match chars[i] {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        '\\' => '\\',
        '"' => '"',
        '\'' => '\'',
        other => {
            return Err(LexError {
                message: format!("unknown escape: \\{other}"),
                span: Span { start: i, end: i + 1 },
            });
        }
    };
    Ok((ch, i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_identifiers() {
        let toks = lex("hello world").unwrap();
        assert_eq!(toks.len(), 2);
        assert!(matches!(&toks[0].kind, TokenKind::Ident(s) if s == "hello"));
    }

    #[test]
    fn lex_keywords() {
        let toks = lex("fn mu match let").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Fn);
        assert_eq!(toks[1].kind, TokenKind::Mu);
        assert_eq!(toks[2].kind, TokenKind::Match);
        assert_eq!(toks[3].kind, TokenKind::Let);
    }

    #[test]
    fn lex_int() {
        let toks = lex("42").unwrap();
        assert!(matches!(toks[0].kind, TokenKind::Int(42)));
    }

    #[test]
    fn lex_float() {
        let toks = lex("3.14").unwrap();
        assert!(matches!(toks[0].kind, TokenKind::Float(_)));
    }

    #[test]
    fn lex_string() {
        let toks = lex("\"hello\\nworld\"").unwrap();
        assert!(matches!(&toks[0].kind, TokenKind::Str(s) if s == "hello\nworld"));
    }

    #[test]
    fn lex_comments() {
        let toks = lex("// line comment\n42").unwrap();
        assert_eq!(toks.len(), 1);
        let toks = lex("/* nested /* comment */ */ 42").unwrap();
        assert_eq!(toks.len(), 1);
    }

    #[test]
    fn lex_unicode_operators() {
        let toks = lex("⊗ ⅋ ⊥").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Tensor);
        assert_eq!(toks[1].kind, TokenKind::Par);
        assert_eq!(toks[2].kind, TokenKind::Bot);
    }

    #[test]
    fn lex_operators() {
        let toks = lex("+ - * / % == != <= >= -> :: @").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Plus);
        assert_eq!(toks[3].kind, TokenKind::Slash);
        assert_eq!(toks[5].kind, TokenKind::EqEq);
        assert_eq!(toks[7].kind, TokenKind::Le);
        assert_eq!(toks[9].kind, TokenKind::Arrow);
        assert_eq!(toks[10].kind, TokenKind::ColonColon);
        assert_eq!(toks[11].kind, TokenKind::At);
    }

    #[test]
    fn lex_reverse_arrow() {
        let toks = lex("<-").unwrap();
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::ReverseArrow);
    }

    #[test]
    fn lex_error_unterminated_string() {
        assert!(lex("\"abc").is_err());
    }

    #[test]
    fn lex_error_bad_char() {
        assert!(lex("#").is_err());
    }
}
