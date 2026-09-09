//! Recursive descent parser for Slant.

use crate::ast::*;
use crate::token::{Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
    in_block: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, errors: Vec::new(), in_block: false }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, ParseError> {
        if self.peek_kind() == Some(&kind) {
            Ok(self.next().unwrap())
        } else {
            let span = self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
            Err(ParseError {
                message: format!(
                    "expected {what}, found {}",
                    self.peek_kind()
                        .map(|k| format!("{k}"))
                        .unwrap_or_else(|| "end of input".into())
                ),
                span,
            })
        }
    }

    /// Skip tokens until we find a synchronization point (semicolon or closing brace).
    fn synchronize(&mut self) {
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::Semicolon | TokenKind::RBrace => {
                    self.pos += 1;
                    return;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, Vec<ParseError>> {
        let mut decls = Vec::new();
        while self.peek().is_some() {
            match self.parse_decl() {
                Ok(d) => decls.push(d),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }
        if self.errors.is_empty() { Ok(Program { decls }) } else { Err(self.errors.clone()) }
    }

    fn parse_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let start = self.pos;
        match self.peek_kind() {
            Some(TokenKind::Struct) => self.parse_struct(),
            Some(TokenKind::Enum) => self.parse_enum(),
            Some(TokenKind::Fn) => {
                // fn( — lambda expression, not declaration
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::LParen) {
                    let e = self.parse_expr()?;
                    Ok(Node {
                        span: e.span,
                        kind: Decl::Fn {
                            name: "main".into(),
                            type_params: vec![],
                            params: vec![],
                            return_type: None,
                            body: e,
                        },
                    })
                } else {
                    self.parse_fn()
                }
            }
            Some(TokenKind::Command) => self.parse_command_decl(),
            Some(TokenKind::Const) => self.parse_const_decl(),
            _ => {
                // Expression as top-level (for scripting)
                let e = self.parse_expr()?;
                let end = self.pos;
                let _ = self.eat(&TokenKind::Semicolon);
                Ok(Node {
                    span: Span { start, end },
                    kind: Decl::Fn {
                        name: "main".into(),
                        type_params: vec![],
                        params: vec![],
                        return_type: None,
                        body: e,
                    },
                })
            }
        }
    }

    fn parse_struct(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Struct, "`struct`")?;
        let name = self.expect_ident("struct name")?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let f = self.expect_ident("field name")?;
            self.expect(TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            fields.push((f, ty.kind));
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RBrace, "`}`")?;
                break;
            }
        }
        Ok(Node { span: t.span, kind: Decl::Struct { name, fields } })
    }

    fn parse_enum(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Enum, "`enum`")?;
        let name = self.expect_ident("enum name")?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut variants = Vec::new();
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let v = self.expect_ident("variant name")?;
            let mut fields = Vec::new();
            if self.eat(&TokenKind::LParen) {
                loop {
                    if self.eat(&TokenKind::RParen) {
                        break;
                    }
                    let ty = self.parse_type()?;
                    fields.push(ty.kind);
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(TokenKind::RParen, "`)`")?;
                        break;
                    }
                }
            }
            variants.push((v, fields));
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RBrace, "`}`")?;
                break;
            }
        }
        Ok(Node { span: t.span, kind: Decl::Enum { name, variants } })
    }

    fn parse_fn(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Fn, "`fn`")?;
        let name = self.expect_ident("function name")?;
        let type_params = self.parse_type_params()?;
        let params = self.parse_params()?;
        let return_type =
            if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?.kind) } else { None };
        let body = self.parse_block()?;
        Ok(Node { span: t.span, kind: Decl::Fn { name, type_params, params, return_type, body } })
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();
        if !self.eat(&TokenKind::Lt) {
            return Ok(params);
        }
        loop {
            if self.eat(&TokenKind::Gt) {
                break;
            }
            params.push(self.expect_ident("type parameter")?);
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::Gt, "`>` after type parameters")?;
                break;
            }
        }
        Ok(params)
    }

    fn parse_command_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Command, "`command`")?;
        let name = self.expect_ident("command name")?;
        let params = self.parse_params()?;
        let body = self.parse_block()?;
        Ok(Node { span: t.span, kind: Decl::Command { name, params, body } })
    }

    fn parse_const_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Const, "`const`")?;
        let name = self.expect_ident("constant name")?;
        self.expect(TokenKind::Colon, "`:` in constant declaration")?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Assign, "`=` in constant declaration")?;
        let value = self.parse_expr()?;
        let end = self.pos;
        self.eat(&TokenKind::Semicolon);
        Ok(Node {
            span: Span { start: t.span.start, end },
            kind: Decl::Const { name, ty: ty.kind, value },
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        self.expect(TokenKind::LParen, "`(`")?;
        loop {
            if self.eat(&TokenKind::RParen) {
                break;
            }
            // to k: -T  — continuation parameter
            if self.eat(&TokenKind::To) {
                let name = self.expect_ident("continuation name")?;
                self.expect(TokenKind::Colon, "`:`")?;
                let ty = self.parse_type()?;
                params.push(Param { name, ty: ty.kind, is_continuation: true });
            } else {
                let name = self.expect_ident("parameter name")?;
                self.expect(TokenKind::Colon, "`:`")?;
                let ty = self.parse_type()?;
                params.push(Param { name, ty: ty.kind, is_continuation: false });
            }
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RParen, "`)`")?;
                break;
            }
        }
        Ok(params)
    }

    /// Parse the two partial-agent forms:
    ///
    /// - `step.to(k, h)` — connect output ports first (`Service`)
    /// - `f.partial(a)` — connect input ports first (`Job`)
    fn parse_partial(&mut self) -> Result<PartialKind, ParseError> {
        let name = self.expect_ident("partial application method")?;
        match name.as_str() {
            "to" => Ok(PartialKind::Service),
            "partial" => Ok(PartialKind::Job),
            other => Err(ParseError {
                message: format!("expected `.to` or `.partial`, found `.{other}`"),
                span: self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 }),
            }),
        }
    }

    fn parse_block(&mut self) -> Result<Node<Expr>, ParseError> {
        let start = self.pos;
        self.expect(TokenKind::LBrace, "`{`")?;
        let outer_in_block = self.in_block;
        self.in_block = true;
        let mut exprs: Vec<Node<Expr>> = Vec::new();
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let e = self.parse_expr()?;
            self.eat(&TokenKind::Semicolon);
            exprs.push(e);
        }
        // Chain lets: `let x = v; rest` makes rest the let's body, so
        // scoping survives lowering.
        let mut kind = if exprs.is_empty() { Expr::Int(0) } else { Expr::Block(exprs) };
        // Wrap from the inside out: each trailing Let captures the rest.
        // (Handled during lowering: Block flattens Lets by nesting.)
        let _ = &mut kind;
        self.in_block = outer_in_block;
        Ok(Node { span: Span { start, end: self.pos }, kind })
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, ParseError> {
        match self.peek_kind().cloned() {
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                Ok(s)
            }
            Some(TokenKind::To) => {
                self.pos += 1;
                Ok("to".into())
            }
            other => {
                let span = self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
                Err(ParseError {
                    message: format!(
                        "expected {what}, found {}",
                        other.map(|k| format!("{k}")).unwrap_or_else(|| "end of input".into())
                    ),
                    span,
                })
            }
        }
    }

    pub fn parse_type(&mut self) -> Result<Node<TypeExpr>, ParseError> {
        let start = self.pos;
        let kind = match self.peek_kind().cloned() {
            Some(TokenKind::Plus) => {
                self.pos += 1;
                let inner = self.parse_type()?;
                TypeExpr::Positive(Box::new(inner))
            }
            Some(TokenKind::Minus) => {
                self.pos += 1;
                let inner = self.parse_type()?;
                TypeExpr::Negative(Box::new(inner))
            }
            Some(TokenKind::LParen) => {
                self.pos += 1;
                let left = self.parse_type()?;
                if self.eat(&TokenKind::Arrow) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Fun(
                        Box::new(left),
                        Box::new(Node { span: Span { start, end: self.pos }, kind: right.kind }),
                    )
                } else if self.eat(&TokenKind::Tensor) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Tensor(
                        Box::new(left),
                        Box::new(Node { span: Span { start, end: self.pos }, kind: right.kind }),
                    )
                } else if self.eat(&TokenKind::Par) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Par(
                        Box::new(left),
                        Box::new(Node { span: Span { start, end: self.pos }, kind: right.kind }),
                    )
                } else {
                    self.expect(TokenKind::RParen, "`)`")?;
                    left.kind
                }
            }
            Some(TokenKind::LBracket) => {
                self.pos += 1;
                let inner = self.parse_type()?;
                self.expect(TokenKind::RBracket, "`]`")?;
                TypeExpr::List(Box::new(inner))
            }
            Some(TokenKind::Dual) => {
                self.pos += 1;
                self.expect(TokenKind::LParen, "`(`")?;
                let inner = self.parse_type()?;
                self.expect(TokenKind::RParen, "`)`")?;
                TypeExpr::Dual(Box::new(inner))
            }
            Some(TokenKind::Bot) => {
                self.pos += 1;
                TypeExpr::Bottom
            }
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                // `Command<I, O>` is a type former, not a base name.
                if s == "Command" && self.eat(&TokenKind::Lt) {
                    let input = self.parse_type()?;
                    self.expect(TokenKind::Comma, "`,` in `Command<I, O>`")?;
                    let output = self.parse_type()?;
                    self.expect(TokenKind::Gt, "`>` after `Command<I, O>`")?;
                    TypeExpr::Command(Box::new(input), Box::new(output))
                } else {
                    TypeExpr::Base(s)
                }
            }
            other => {
                let span = self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
                return Err(ParseError {
                    message: format!(
                        "expected type, found {}",
                        other.map(|k| format!("{k}")).unwrap_or_else(|| "end of input".into())
                    ),
                    span,
                });
            }
        };
        Ok(Node { span: Span { start, end: self.pos }, kind })
    }

    pub fn parse_expr(&mut self) -> Result<Node<Expr>, ParseError> {
        self.parse_binary(0)
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Node<Expr>, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let (op, prec) = match self.peek_kind() {
                Some(TokenKind::Plus) => (BinOp::Add, 1),
                Some(TokenKind::Minus) => (BinOp::Sub, 1),
                Some(TokenKind::Star) | Some(TokenKind::Tensor) => (BinOp::Mul, 2),
                Some(TokenKind::Slash) => (BinOp::Div, 2),
                Some(TokenKind::Percent) => (BinOp::Mod, 2),
                Some(TokenKind::EqEq) => (BinOp::Eq, 0),
                Some(TokenKind::NotEq) => (BinOp::Ne, 0),
                Some(TokenKind::Lt) => (BinOp::Lt, 0),
                Some(TokenKind::Gt) => (BinOp::Gt, 0),
                Some(TokenKind::Le) => (BinOp::Le, 0),
                Some(TokenKind::Ge) => (BinOp::Ge, 0),
                Some(TokenKind::AmpAmp) => (BinOp::And, 0),
                Some(TokenKind::PipePipe) => (BinOp::Or, 0),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.pos += 1;
            let rhs = self.parse_binary(prec + 1)?;
            let span = Span { start: lhs.span.start, end: rhs.span.end };
            lhs = Node { span, kind: Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) } };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Node<Expr>, ParseError> {
        if self.eat(&TokenKind::Bang) {
            let start = self.pos - 1;
            let body = self.parse_unary()?;
            let end = self.pos;
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::UnOp { op: UnOp::Not, body: Box::new(body) },
            });
        }
        if self.eat(&TokenKind::Minus) {
            let start = self.pos;
            let body = self.parse_unary()?;
            let end = self.pos;
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::UnOp { op: UnOp::Neg, body: Box::new(body) },
            });
        }
        if self.eat(&TokenKind::Dual) {
            let start = self.pos;
            let body = self.parse_unary()?;
            let end = self.pos;
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::Dual { body: Box::new(body) },
            });
        }
        if self.eat(&TokenKind::Spawn) {
            let start = self.pos;
            let body = self.parse_unary()?;
            let end = self.pos;
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::Spawn { body: Box::new(body) },
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Node<Expr>, ParseError> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek_kind() {
                Some(TokenKind::LParen) => {
                    self.pos += 1;
                    let mut args = Vec::new();
                    loop {
                        if self.eat(&TokenKind::RParen) {
                            break;
                        }
                        args.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            self.expect(TokenKind::RParen, "`)`")?;
                            break;
                        }
                    }
                    let end = self.pos;
                    e = Node {
                        span: Span { start: e.span.start, end },
                        kind: Expr::Call { callee: Box::new(e), args },
                    };
                }
                Some(TokenKind::Question) => {
                    self.pos += 1;
                    let end = self.pos;
                    e = Node {
                        span: Span { start: e.span.start, end },
                        kind: Expr::ErrorProp { expr: Box::new(e) },
                    };
                }
                Some(TokenKind::Dot) => {
                    self.pos += 1;
                    match self.parse_partial()? {
                        PartialKind::Service => {
                            self.expect(TokenKind::LParen, "`(` after `.to`")?;
                            let mut continuations = Vec::new();
                            loop {
                                if self.eat(&TokenKind::RParen) {
                                    break;
                                }
                                continuations.push(self.parse_expr()?);
                                if !self.eat(&TokenKind::Comma) {
                                    self.expect(TokenKind::RParen, "`)`")?;
                                    break;
                                }
                            }
                            let end = self.pos;
                            e = Node {
                                span: Span { start: e.span.start, end },
                                kind: Expr::Service { agent: Box::new(e), continuations },
                            };
                        }
                        PartialKind::Job => {
                            self.expect(TokenKind::LParen, "`(` after `.partial`")?;
                            let mut values = Vec::new();
                            loop {
                                if self.eat(&TokenKind::RParen) {
                                    break;
                                }
                                values.push(self.parse_expr()?);
                                if !self.eat(&TokenKind::Comma) {
                                    self.expect(TokenKind::RParen, "`)`")?;
                                    break;
                                }
                            }
                            let end = self.pos;
                            e = Node {
                                span: Span { start: e.span.start, end },
                                kind: Expr::Job { agent: Box::new(e), values },
                            };
                        }
                    }
                }
                Some(TokenKind::At) => {
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    e = Node {
                        span: Span { start: e.span.start, end: rhs.span.end },
                        kind: Expr::Interaction { left: Box::new(e), right: Box::new(rhs) },
                    };
                }
                Some(TokenKind::LBracket) => {
                    self.pos += 1;
                    let start_expr = if self.peek_kind() == Some(&TokenKind::DotDot) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    let mut end_expr = None;
                    let mut is_range = false;
                    if self.eat(&TokenKind::DotDot) {
                        is_range = true;
                        if self.peek_kind() != Some(&TokenKind::RBracket) {
                            end_expr = Some(Box::new(self.parse_expr()?));
                        }
                    }
                    self.expect(TokenKind::RBracket, "`]`")?;
                    let end = self.pos;
                    e = if let Some(index) = (!is_range).then(|| start_expr.clone()).flatten() {
                        Node {
                            span: Span { start: e.span.start, end },
                            kind: Expr::Index { value: Box::new(e), index },
                        }
                    } else if is_range {
                        Node {
                            span: Span { start: e.span.start, end },
                            kind: Expr::Slice {
                                value: Box::new(e),
                                start: start_expr,
                                end: end_expr,
                            },
                        }
                    } else {
                        return Err(ParseError {
                            message: "indexing requires an index".into(),
                            span: Span { start: e.span.start, end },
                        });
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Node<Expr>, ParseError> {
        let start = self.pos;
        match self.peek_kind().cloned() {
            Some(TokenKind::Int(n)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Int(n) })
            }
            Some(TokenKind::Float(n)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Float(n) })
            }
            Some(TokenKind::Str(s)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Str(s) })
            }
            Some(TokenKind::Char(c)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Char(c) })
            }
            Some(TokenKind::Bool(b)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Bool(b) })
            }
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Ident(s) })
            }
            Some(TokenKind::Fn) => {
                self.pos += 1;
                // fn(x: T) -> R { body }
                self.expect(TokenKind::LParen, "`(`")?;
                let param = self.expect_ident("parameter name")?;
                let param_type =
                    if self.eat(&TokenKind::Colon) { Some(self.parse_type()?.kind) } else { None };
                self.expect(TokenKind::RParen, "`)`")?;
                let return_type =
                    if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?.kind) } else { None };
                let body = self.parse_block()?;
                Ok(Node {
                    span: Span { start, end: self.pos },
                    kind: Expr::Lambda { param, param_type, return_type, body: Box::new(body) },
                })
            }
            Some(TokenKind::Mu) => {
                self.pos += 1;
                // mu(k: -T) { body }
                // mu() -> T { body }
                // mu(x: +T) -> R { body }
                self.expect(TokenKind::LParen, "`(`")?;
                let binder = if self.eat(&TokenKind::RParen) {
                    None
                } else {
                    let name = self.expect_ident("binder name")?;
                    let ty = if self.eat(&TokenKind::Colon) {
                        Some(self.parse_type()?.kind)
                    } else {
                        None
                    };
                    self.expect(TokenKind::RParen, "`)`")?;
                    Some((name, ty))
                };
                let return_type =
                    if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?.kind) } else { None };
                let body = self.parse_block()?;
                Ok(Node {
                    span: Span { start, end: self.pos },
                    kind: Expr::Mu { binder, return_type, body: Box::new(body) },
                })
            }
            Some(TokenKind::Match) => {
                self.pos += 1;
                let scrutinee = self.parse_expr()?;
                self.expect(TokenKind::LBrace, "`{`")?;
                let mut arms = Vec::new();
                loop {
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    let pattern = self.parse_pattern()?;
                    let guard =
                        if self.eat(&TokenKind::If) { Some(self.parse_expr()?) } else { None };
                    self.expect(TokenKind::FatArrow, "`=>`")?;
                    let body = self.parse_expr()?;
                    self.eat(&TokenKind::Comma);
                    arms.push(MatchArm { pattern, guard, body });
                }
                Ok(Node {
                    span: Span { start, end: self.pos },
                    kind: Expr::Match { scrutinee: Box::new(scrutinee), arms },
                })
            }
            Some(TokenKind::Let) => {
                self.pos += 1;
                let name = self.expect_ident("binding name")?;
                let ty =
                    if self.eat(&TokenKind::Colon) { Some(self.parse_type()?.kind) } else { None };
                let value = if self.eat(&TokenKind::Assign) {
                    self.parse_expr()?
                } else {
                    return Err(ParseError {
                        message: "expected `=` in let binding".into(),
                        span: Span { start, end: self.pos },
                    });
                };
                let body = if !self.in_block && self.eat(&TokenKind::Semicolon) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                Ok(Node {
                    span: Span { start, end: self.pos },
                    kind: Expr::Let { name, ty, value: Box::new(value), body },
                })
            }
            Some(TokenKind::If) => {
                self.pos += 1;
                let cond = self.parse_expr()?;
                let then = self.parse_block()?;
                let otherwise = if self.eat(&TokenKind::Else) {
                    if self.peek_kind() == Some(&TokenKind::If) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        Some(Box::new(self.parse_block()?))
                    }
                } else {
                    None
                };
                Ok(Node {
                    span: Span { start, end: self.pos },
                    kind: Expr::If { cond: Box::new(cond), then: Box::new(then), otherwise },
                })
            }
            Some(TokenKind::LParen) => {
                self.pos += 1;
                // () or (e) or (e1, e2)
                if self.eat(&TokenKind::RParen) {
                    return Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Int(0) });
                }
                let first = self.parse_expr()?;
                if self.eat(&TokenKind::Comma) {
                    let mut items = vec![first];
                    loop {
                        if self.eat(&TokenKind::RParen) {
                            break;
                        }
                        items.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            self.expect(TokenKind::RParen, "`)`")?;
                            break;
                        }
                    }
                    Ok(Node { span: Span { start, end: self.pos }, kind: Expr::Pair(items) })
                } else {
                    self.expect(TokenKind::RParen, "`)`")?;
                    Ok(Node { span: Span { start, end: self.pos }, kind: first.kind })
                }
            }
            Some(TokenKind::LBrace) => self.parse_block(),
            other => {
                let span = self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
                Err(ParseError {
                    message: format!(
                        "expected expression, found {}",
                        other.map(|k| format!("{k}")).unwrap_or_else(|| "end of input".into())
                    ),
                    span,
                })
            }
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let first = self.parse_single_pattern()?;
        if self.peek_kind() == Some(&TokenKind::DotDotEq) {
            self.pos += 1;
            let end = self.parse_single_pattern()?;
            if self.peek_kind() == Some(&TokenKind::Pipe) {
                let mut alternatives =
                    vec![Pattern::Range { start: Box::new(first), end: Box::new(end) }];
                while self.eat(&TokenKind::Pipe) {
                    alternatives.push(self.parse_pattern()?);
                }
                return Ok(Pattern::Or(alternatives));
            }
            return Ok(Pattern::Range { start: Box::new(first), end: Box::new(end) });
        }
        if self.peek_kind() == Some(&TokenKind::Pipe) {
            let mut alternatives = vec![first];
            while self.eat(&TokenKind::Pipe) {
                alternatives.push(self.parse_single_pattern()?);
            }
            return Ok(Pattern::Or(alternatives));
        }
        if self.peek_kind() == Some(&TokenKind::At)
            && let Pattern::Ident(name) = first
        {
            self.pos += 1;
            let pattern = self.parse_pattern()?;
            return Ok(Pattern::Binding { name, pattern: Box::new(pattern) });
        }
        Ok(first)
    }

    fn parse_single_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.peek_kind().cloned() {
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                if s == "_" {
                    return Ok(Pattern::Wildcard);
                }
                // Path pattern: Enum::Variant or Enum::Variant(fields)
                if self.peek_kind() == Some(&TokenKind::ColonColon) {
                    self.pos += 1;
                    let variant = self.expect_ident("variant name")?;
                    let mut fields = Vec::new();
                    if self.eat(&TokenKind::LParen) {
                        loop {
                            if self.eat(&TokenKind::RParen) {
                                break;
                            }
                            fields.push(self.parse_pattern()?);
                            if !self.eat(&TokenKind::Comma) {
                                self.expect(TokenKind::RParen, "`)`")?;
                                break;
                            }
                        }
                    }
                    return Ok(Pattern::Enum { name: s, variant, fields });
                }
                // Check for enum pattern: Name(variant)
                if self.peek_kind() == Some(&TokenKind::LParen) {
                    self.pos += 1;
                    let mut fields = Vec::new();
                    loop {
                        if self.eat(&TokenKind::RParen) {
                            break;
                        }
                        fields.push(self.parse_pattern()?);
                        if !self.eat(&TokenKind::Comma) {
                            self.expect(TokenKind::RParen, "`)`")?;
                            break;
                        }
                    }
                    return Ok(Pattern::Enum { name: s, variant: String::new(), fields });
                }
                Ok(Pattern::Ident(s))
            }
            Some(TokenKind::Int(n)) => {
                self.pos += 1;
                Ok(Pattern::Int(n))
            }
            Some(TokenKind::Str(s)) => {
                self.pos += 1;
                Ok(Pattern::Str(s))
            }
            Some(TokenKind::Char(c)) => {
                self.pos += 1;
                Ok(Pattern::Char(c))
            }
            Some(TokenKind::Float(n)) => {
                self.pos += 1;
                Ok(Pattern::Float(n))
            }
            Some(TokenKind::DotDot) => {
                self.pos += 1;
                Ok(Pattern::Rest)
            }
            Some(TokenKind::Bool(b)) => {
                self.pos += 1;
                Ok(Pattern::Bool(b))
            }
            Some(TokenKind::LParen) => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    if self.eat(&TokenKind::RParen) {
                        break;
                    }
                    items.push(self.parse_pattern()?);
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(TokenKind::RParen, "`)`")?;
                        break;
                    }
                }
                Ok(Pattern::Tuple(items))
            }
            other => {
                let span = self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
                Err(ParseError {
                    message: format!(
                        "expected pattern, found {}",
                        other.map(|k| format!("{k}")).unwrap_or_else(|| "end of input".into())
                    ),
                    span,
                })
            }
        }
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, Vec<ParseError>> {
    let mut p = Parser::new(tokens);
    p.parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_str(s: &str) -> Program {
        let toks = lex(s).unwrap();
        parse(toks).unwrap()
    }

    #[test]
    fn parse_int_expr() {
        let p = parse_str("42");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_fn_def() {
        let p = parse_str("fn add(x: +i32, y: +i32) -> i32 { x + y }");
        assert_eq!(p.decls.len(), 1);
        let d = &p.decls[0];
        assert!(
            matches!(&d.kind, Decl::Fn { name, params, .. } if name == "add" && params.len() == 2)
        );
    }

    #[test]
    fn parse_mu_simple() {
        let p = parse_str("mu(k: -i32) { k(42) }");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_mu_return() {
        let p = parse_str("mu() -> i32 { 42 }");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_mu_param() {
        let p = parse_str("mu(x: +i32) -> i32 { x + 1 }");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_command_def() {
        let p = parse_str("command step(x: +i32, to k: -i32) { k(x) }");
        assert_eq!(p.decls.len(), 1);
        let d = &p.decls[0];
        assert!(
            matches!(&d.kind, Decl::Command { name, params, .. } if name == "step" && params.len() == 2)
        );
    }

    #[test]
    fn parse_lambda() {
        let p = parse_str("fn(x: +i32) -> i32 { x + 1 }");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_match() {
        let p = parse_str("match x { _ => 0, }");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_let() {
        let p = parse_str("let x = 42; x");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_call() {
        let p = parse_str("f(1, 2)");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_error_prop() {
        let p = parse_str("read_file(path)?");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_interaction() {
        let p = parse_str("f @ dual(42)");
        assert_eq!(p.decls.len(), 1);
    }

    #[test]
    fn parse_command_type() {
        let src = "fn parse(text: Command<i64, +i64>) -> i64 { 1 }";
        let toks = lex(src).unwrap();
        let ast = parse(toks).unwrap();
        let Decl::Fn { params, .. } = &ast.decls[0].kind else {
            panic!("expected fn declaration");
        };
        assert!(matches!(
            &params[0].ty,
            TypeExpr::Command(input, output)
                if matches!(&input.kind, TypeExpr::Base(base) if base == "i64")
                    && matches!(&output.kind, TypeExpr::Positive(inner)
                        if matches!(&inner.kind, TypeExpr::Base(out) if out == "i64"))
        ));
    }

    #[test]
    fn parse_service_partial_application() {
        let p = parse_str("step.to(k, h)");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(
            &body.kind,
            Expr::Service { agent, continuations }
                if matches!(&agent.kind, Expr::Ident(name) if name == "step")
                    && continuations.len() == 2
        ));
    }

    #[test]
    fn parse_job_partial_application() {
        let p = parse_str("f.partial(42)");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(
            &body.kind,
            Expr::Job { agent, values }
                if matches!(&agent.kind, Expr::Ident(name) if name == "f")
                    && values.len() == 1
        ));
    }

    #[test]
    fn parse_rejects_unknown_partial_method() {
        let toks = lex("f.whatever(42)").unwrap();
        assert!(parse(toks).is_err());
    }

    #[test]
    fn parse_else_if_chain() {
        let p = parse_str("if a { 1 } else if b { 2 } else if c { 3 } else { 4 }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(&body.kind, Expr::If { .. }));
        let Expr::If { otherwise: Some(outer), .. } = &body.kind else {
            panic!("expected else");
        };
        assert!(matches!(&outer.kind, Expr::If { .. }));
    }

    #[test]
    fn parse_else_if_without_final_else() {
        let p = parse_str("if a { 1 } else if b { 2 }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(
            &body.kind,
            Expr::If { otherwise: Some(outer), .. }
                if matches!(&outer.kind, Expr::If { otherwise: None, .. })
        ));
    }

    #[test]
    fn parse_boolean_and_unary_operators() {
        let p = parse_str("!a && b || c");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(&body.kind, Expr::BinOp { op: BinOp::Or, .. }));
    }

    #[test]
    fn parse_index_and_slice() {
        let p = parse_str("input[pos]");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(&body.kind, Expr::Index { .. }));

        let p = parse_str("input[start..end]");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(&body.kind, Expr::Slice { .. }));
    }

    #[test]
    fn parse_match_guard_or_range_binding() {
        let p = parse_str("match c { c @ '0'..='9' if c < 'a' => 1, 'x' | 'y' => 2, _ => 3 }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        let Expr::Match { arms, .. } = &body.kind else {
            panic!("expected match");
        };
        assert!(matches!((&arms[0].pattern, &arms[0].guard), (Pattern::Binding { .. }, Some(_))));
        assert!(matches!(&arms[1].pattern, Pattern::Or(_)));
    }

    #[test]
    fn parse_const_decl() {
        let p = parse_str("const X: +char = 'x';");
        assert!(matches!(&p.decls[0].kind, Decl::Const { name, .. } if name == "X"));
    }

    #[test]
    fn parse_typed_let() {
        let p = parse_str("let x: +i32 = 42; x");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        assert!(matches!(&body.kind, Expr::Let { ty: Some(_), .. }));
    }
}
