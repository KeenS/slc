//! A parser for the pretty-printed core IR.
//!
//! Pretty-printing is the core's external syntax, so it has to be exact: what
//! `Display` writes, this module reads back. Every pretty-printed term,
//! co-term, command, and type round-trips through here unchanged, which is
//! what keeps the printed form a faithful view of the IR rather than an
//! approximation of it.

use crate::command::Command;
use crate::coterm::{CoCaseBranch, CoTerm};
use crate::term::{CoMatchBranch, Term};
use crate::types::{Base, Effect, Row, Type};

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
                // `μ[M; .d(α). c | …]` is a menu; `μα. c` binds a co-variable.
                if self.eat("[") {
                    let written_owner = if self.eat(".") {
                        None
                    } else {
                        let owner = self.name()?;
                        if self.eat("]") {
                            return Ok(Term::CoMatch { owner, branches: Vec::new() });
                        }
                        self.expect(";")?;
                        self.expect(".")?;
                        Some(owner)
                    };
                    let mut branches = Vec::new();
                    loop {
                        let label = self.name()?;
                        self.expect("(")?;
                        let binder = self.name()?;
                        self.expect(")")?;
                        self.expect(".")?;
                        branches.push(CoMatchBranch {
                            label,
                            binder,
                            body: Box::new(self.command()?),
                        });
                        if self.eat("|") {
                            self.expect(".")?;
                            continue;
                        }
                        self.expect("]")?;
                        break;
                    }
                    let owner = written_owner
                        .or_else(|| {
                            branches[0].label.rsplit_once("::").map(|(owner, _)| owner.to_string())
                        })
                        .ok_or_else(|| ParseError {
                            message: "menu destructor label must include its owner".into(),
                            at: self.pos,
                        })?;
                    return Ok(Term::CoMatch { owner, branches });
                }
                let a = self.name()?;
                self.expect(".")?;
                Ok(Term::Mu(a, Box::new(self.command()?)))
            }
            Some('(') => {
                self.pos += 1;
                let mut items = vec![self.term()?];
                self.expect("⊗")?;
                items.push(self.term()?);
                while self.eat("⊗") {
                    items.push(self.term()?);
                }
                self.expect(")")?;
                Ok(Term::Tuple(items))
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
                    let owner = self.name()?;
                    if self.eat("]") {
                        return Ok(CoTerm::CoCase { owner, branches: Vec::new() });
                    }
                    self.expect(";")?;
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
                    return Ok(CoTerm::CoCase { owner, branches });
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
            // `.d(e)` is a destructor: a request carrying the continuation
            // that wants its answer.
            Some('.') => {
                self.pos += 1;
                let label = self.name()?;
                self.expect("(")?;
                let e = self.coterm()?;
                self.expect(")")?;
                Ok(CoTerm::Dtor(label, Box::new(e)))
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
                self.atom()
            }
            Some('-') => {
                self.pos += 1;
                Ok(self.atom()?.dual())
            }
            Some('(') => {
                self.pos += 1;
                // `(-> T / {E})` — a by-name computation. The blank domain is
                // the unit of the arrow.
                if self.eat("->") {
                    let inner = self.ty()?;
                    self.expect("/")?;
                    let row = self.row()?;
                    self.expect(")")?;
                    return Ok(Type::delayed(inner, row));
                }
                // A paren holding only its separator is that connective's unit.
                for (unit, ty) in
                    [(",", Type::ONE), (";", Type::BOTTOM), ("&", Type::TOP), ("|", Type::ZERO)]
                {
                    if self.eat(unit) {
                        return self.finish_type_group(ty);
                    }
                }
                let left = self.ty()?;
                self.spaces();
                // `(A hn B / {E} / {F})` — a handler value.
                if self.eat("hn") {
                    let answer = self.ty()?;
                    self.expect("/")?;
                    let discharged = self.row()?;
                    let residual = if self.eat("/") { self.row()? } else { Row::default() };
                    self.expect(")")?;
                    return Ok(Type::Named(
                        "Handler".into(),
                        vec![
                            left,
                            answer,
                            Type::rowed(Type::ONE, discharged),
                            Type::rowed(Type::ONE, residual),
                        ],
                    ));
                }
                if matches!(self.peek(), Some('/' | ')')) {
                    return self.finish_type_group(left);
                }
                let separator = ["->", ",", ";", "&", "|"]
                    .into_iter()
                    .find(|separator| self.eat(separator))
                    .ok_or_else(|| self.error("expected a type connective"))?;
                let right = self.ty()?;
                if separator == "->" {
                    // `A -> B` is `(dual(A) ; B)`.
                    return self.finish_type_group(Type::arrow(left, right));
                }
                let mut items = vec![left, right];
                while self.eat(separator) {
                    items.push(self.ty()?);
                }
                self.finish_type_group(match separator {
                    "," => Type::Tensor(items),
                    ";" => Type::Par(items),
                    "&" => Type::With(items),
                    _ => Type::Sum(items),
                })
            }
            // `%i` — a declaration's type parameter, by position.
            Some('%') => {
                self.pos += 1;
                let digits = self.name()?;
                digits
                    .parse()
                    .map(Type::Param)
                    .map_err(|_| self.error("expected a type-parameter index"))
            }
            Some(_) => {
                let name = self.name()?;
                if name == "dual" {
                    self.expect("(")?;
                    let inner = self.ty()?;
                    self.expect(")")?;
                    return Ok(Type::Dual(Box::new(inner)));
                }
                if name == "Delayed" {
                    self.expect("<")?;
                    let inner = self.ty()?;
                    self.expect(",")?;
                    let row = self.row()?;
                    self.expect(">")?;
                    return Ok(Type::delayed(inner, row));
                }
                // `Name<A, B>` — a declaration applied to type arguments.
                let mut args = Vec::new();
                if self.eat("<") {
                    loop {
                        args.push(self.ty()?);
                        if self.eat(",") {
                            continue;
                        }
                        self.expect(">")?;
                        break;
                    }
                }
                Ok(Type::Named(name, args))
            }
            None => Err(self.error("expected a type")),
        }
    }

    fn finish_type_group(&mut self, mut ty: Type) -> Result<Type, ParseError> {
        while self.eat("/") {
            ty = Type::rowed(ty, self.row()?);
        }
        self.expect(")")?;
        Ok(ty)
    }

    fn row(&mut self) -> Result<Row, ParseError> {
        self.expect("{")?;
        let mut row = Row::default();
        if self.eat("}") {
            return Ok(row);
        }
        loop {
            if self.eat("..?") {
                if row.tail.is_some() {
                    return Err(self.error("an effect row has at most one tail"));
                }
                row.tail = Some(
                    self.name()?
                        .parse()
                        .map_err(|_| self.error("expected a row variable index"))?,
                );
            } else {
                let name = self.name()?;
                let mut args = Vec::new();
                if self.eat("<") {
                    loop {
                        args.push(self.ty()?);
                        if !self.eat(",") {
                            self.expect(">")?;
                            break;
                        }
                    }
                }
                row.effects.insert(Effect { name, args });
            }
            if !self.eat(",") {
                self.expect("}")?;
                return Ok(row);
            }
        }
    }

    fn atom(&mut self) -> Result<Type, ParseError> {
        let name = self.name()?;
        Ok(match name.as_str() {
            "i32" => Type::Pos(Base::I32),
            "i8" => Type::Pos(Base::I8),
            "i64" => Type::Pos(Base::I64),
            "u8" => Type::Pos(Base::U8),
            "u32" => Type::Pos(Base::U32),
            "u64" => Type::Pos(Base::U64),
            "f32" => Type::Pos(Base::F32),
            "f64" => Type::Pos(Base::F64),
            "String" => Type::Pos(Base::Str),
            "char" => Type::Pos(Base::Char),
            "unit" => Type::ONE,
            "File" => Type::Pos(Base::File),
            _ => return Err(self.error(&format!("unknown base type `{name}`"))),
        })
    }
}
