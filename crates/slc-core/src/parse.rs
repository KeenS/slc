//! A parser for the pretty-printed core IR.
//!
//! Pretty-printing is the core's external syntax, so it has to be exact: what
//! `Display` writes, this module reads back. Every pretty-printed term,
//! co-term, command, and type round-trips through here unchanged, which is
//! what keeps the printed form a faithful view of the IR rather than an
//! approximation of it.

use crate::command::Command;
use crate::coterm::{CoCaseBranch, CoTerm};
use crate::term::Term;
use crate::types::{Base, Type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub at: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at character {})", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

/// Parse a pretty-printed term.
pub fn parse_term(source: &str) -> Result<Term, ParseError> {
    Parser::over(source).finish(|p| p.term())
}

/// Parse a pretty-printed co-term.
pub fn parse_coterm(source: &str) -> Result<CoTerm, ParseError> {
    Parser::over(source).finish(|p| p.coterm())
}

/// Parse a pretty-printed command.
pub fn parse_command(source: &str) -> Result<Command, ParseError> {
    Parser::over(source).finish(|p| p.command())
}

/// Parse a pretty-printed type.
pub fn parse_type(source: &str) -> Result<Type, ParseError> {
    Parser::over(source).finish(|p| p.ty())
}

/// The combining mark that distinguishes `μ̃` from `μ`.
const TILDE: char = '\u{303}';

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn over(source: &str) -> Self {
        Self { chars: source.chars().collect(), pos: 0 }
    }

    fn finish<T>(
        mut self,
        parse: impl Fn(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        let parsed = parse(&mut self)?;
        self.spaces();
        if self.pos < self.chars.len() {
            return Err(self.error("unexpected trailing input"));
        }
        Ok(parsed)
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError { message: message.to_string(), at: self.pos }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn spaces(&mut self) {
        while self.peek() == Some(' ') {
            self.pos += 1;
        }
    }

    /// Consume `text` if it is next, after skipping spaces.
    fn eat(&mut self, text: &str) -> bool {
        self.spaces();
        let expected: Vec<char> = text.chars().collect();
        if self.chars[self.pos..].starts_with(&expected) {
            self.pos += expected.len();
            return true;
        }
        false
    }

    fn expect(&mut self, text: &str) -> Result<(), ParseError> {
        if self.eat(text) {
            return Ok(());
        }
        Err(self.error(&format!("expected `{text}`")))
    }

    /// A name: an identifier, or one of the lowered constants such as
    /// `$str_"a b"`, whose quoted section may contain anything.
    fn name(&mut self) -> Result<String, ParseError> {
        self.spaces();
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == '"' {
                out.push(c);
                self.pos += 1;
                while let Some(c) = self.peek() {
                    self.pos += 1;
                    out.push(c);
                    if c == '\\' {
                        if let Some(escaped) = self.peek() {
                            self.pos += 1;
                            out.push(escaped);
                        }
                        continue;
                    }
                    if c == '"' {
                        break;
                    }
                }
                continue;
            }
            // A `.` ends a name — it is the binder separator of `λx. t` —
            // unless it is the decimal point of a lowered constant such as
            // `$float_1.5`, where a digit follows it.
            if c == '.' {
                let decimal = out.starts_with('$')
                    && self.chars.get(self.pos + 1).is_some_and(char::is_ascii_digit);
                if !decimal {
                    break;
                }
            }
            if c.is_alphanumeric() || matches!(c, '_' | '$' | ':' | '.' | '-') {
                out.push(c);
                self.pos += 1;
                continue;
            }
            break;
        }
        if out.is_empty() {
            return Err(self.error("expected a name"));
        }
        Ok(out)
    }

    /// A parenthesized, comma-separated list of binders: `(x, y)`, or `()`.
    fn binder_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect("(")?;
        let mut binders = Vec::new();
        if self.eat(")") {
            return Ok(binders);
        }
        loop {
            binders.push(self.name()?);
            if self.eat(",") {
                continue;
            }
            self.expect(")")?;
            return Ok(binders);
        }
    }

    fn term(&mut self) -> Result<Term, ParseError> {
        self.spaces();
        match self.peek() {
            Some('λ') => {
                self.pos += 1;
                let x = self.name()?;
                self.expect(".")?;
                Ok(Term::Lam(x, Box::new(self.term()?)))
            }
            Some('μ') => {
                self.pos += 1;
                let a = self.name()?;
                self.expect(".")?;
                Ok(Term::Mu(a, Box::new(self.command()?)))
            }
            Some('(') => {
                self.pos += 1;
                let left = self.term()?;
                self.expect("⊗")?;
                let right = self.term()?;
                self.expect(")")?;
                Ok(Term::Pair(Box::new(left), Box::new(right)))
            }
            Some(_) => {
                let name = self.name()?;
                match name.as_str() {
                    "co" => {
                        self.expect("(")?;
                        let inner = Box::new(self.coterm()?);
                        self.expect(")")?;
                        Ok(Term::Co(inner))
                    }
                    // A label applied to a payload is an injection; a bare
                    // name is a variable.
                    _ if self.peek() == Some('(') => {
                        self.pos += 1;
                        let payload = Box::new(self.term()?);
                        self.expect(")")?;
                        Ok(Term::Tag(name, payload))
                    }
                    _ => Ok(Term::Var(name)),
                }
            }
            None => Err(self.error("expected a term")),
        }
    }

    fn coterm(&mut self) -> Result<CoTerm, ParseError> {
        self.spaces();
        match self.peek() {
            Some('μ') if self.chars.get(self.pos + 1).copied() == Some(TILDE) => {
                self.pos += 2;
                if self.eat("[") {
                    let mut branches = Vec::new();
                    loop {
                        let label = self.name()?;
                        let binders = self.binder_list()?;
                        self.expect(".")?;
                        branches.push(CoCaseBranch {
                            label,
                            binders,
                            body: Box::new(self.command()?),
                        });
                        if self.eat("|") {
                            continue;
                        }
                        self.expect("]")?;
                        break;
                    }
                    return Ok(CoTerm::CoCase(branches));
                }
                // `μ̃(x, y). c` binds a product; `μ̃x. c` binds one value.
                if self.peek() == Some('(') {
                    let binders = self.binder_list()?;
                    self.expect(".")?;
                    return Ok(CoTerm::MuTildeTensor(binders, Box::new(self.command()?)));
                }
                let x = self.name()?;
                self.expect(".")?;
                Ok(CoTerm::MuTilde(x, Box::new(self.command()?)))
            }
            // A projection, a co-variable, or an application `v · e`: parse
            // a term first — a following `·` makes it the argument.
            Some(_) => {
                let t = self.term()?;
                self.spaces();
                if self.eat("·") {
                    return Ok(CoTerm::App(t, Box::new(self.coterm()?)));
                }
                match t {
                    Term::Var(name) => {
                        // A projection prints as one token `prj:index`.
                        if let Some(rest) = name.strip_prefix("prj:") {
                            return rest
                                .parse()
                                .map(CoTerm::Prj)
                                .map_err(|_| self.error("malformed projection co-term"));
                        }
                        Ok(CoTerm::Covar(name))
                    }
                    _ => Err(self.error("expected a co-term")),
                }
            }
            None => Err(self.error("expected a co-term")),
        }
    }

    fn command(&mut self) -> Result<Command, ParseError> {
        self.spaces();
        self.expect("⟨")?;
        let t = self.term()?;
        self.expect("∥")?;
        let e = self.coterm()?;
        self.expect("⟩")?;
        Ok(Command::Cut(t, e))
    }

    fn ty(&mut self) -> Result<Type, ParseError> {
        self.spaces();
        match self.peek() {
            Some('?') => {
                self.pos += 1;
                let digits = self.name()?;
                digits
                    .parse()
                    .map(Type::Var)
                    .map_err(|_| self.error("expected an inference variable index"))
            }
            Some('+') => {
                self.pos += 1;
                Ok(Type::Pos(self.base()?))
            }
            Some('-') => {
                self.pos += 1;
                Ok(Type::Neg(self.base()?))
            }
            Some('!') => {
                self.pos += 1;
                Ok(Type::Bang(Box::new(self.ty()?)))
            }
            Some('↓') => {
                self.pos += 1;
                Ok(Type::Down(Box::new(self.ty()?)))
            }
            Some('↑') => {
                self.pos += 1;
                Ok(Type::Up(Box::new(self.ty()?)))
            }
            Some('[') => {
                self.pos += 1;
                let inner = self.ty()?;
                self.expect("]")?;
                Ok(Type::List(Box::new(inner)))
            }
            Some('⊥') => {
                self.pos += 1;
                Ok(Type::Bottom)
            }
            Some('1') => {
                self.pos += 1;
                Ok(Type::One)
            }
            Some('(') => {
                self.pos += 1;
                let left = self.ty()?;
                let connective = ["⊗", "⅋", "&", "+", "->"]
                    .into_iter()
                    .find(|connective| self.eat(connective))
                    .ok_or_else(|| self.error("expected a type connective"))?;
                let right = self.ty()?;
                self.expect(")")?;
                let (left, right) = (Box::new(left), Box::new(right));
                Ok(match connective {
                    "⊗" => Type::Tensor(left, right),
                    "⅋" => Type::Par(left, right),
                    "&" => Type::With(left, right),
                    "+" => Type::Sum(left, right),
                    // `A -> B` is `-A ⅋ B`.
                    _ => Type::arrow(*left, *right),
                })
            }
            Some(_) => {
                let name = self.name()?;
                if name == "dual" {
                    self.expect("(")?;
                    let inner = self.ty()?;
                    self.expect(")")?;
                    return Ok(Type::Dual(Box::new(inner)));
                }
                Ok(Type::Named(name))
            }
            None => Err(self.error("expected a type")),
        }
    }

    fn base(&mut self) -> Result<Base, ParseError> {
        let name = self.name()?;
        Ok(match name.as_str() {
            "i32" => Base::I32,
            "i64" => Base::I64,
            "u32" => Base::U32,
            "u64" => Base::U64,
            "bool" => Base::Bool,
            "String" => Base::Str,
            "char" => Base::Char,
            "unit" => Base::Unit,
            "File" => Base::File,
            _ => return Err(self.error(&format!("unknown base type `{name}`"))),
        })
    }
}
