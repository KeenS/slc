//! Recursive descent parser for Slant.

use crate::ast::*;
use crate::token::{Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// Whether the parameters being parsed must carry types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeAnnotations {
    Required,
    Optional,
}

/// Type parameters with their trait bounds.
type TypeParams = (Vec<String>, Vec<(String, String)>);

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
    in_block: bool,
    /// True where a following `{` opens a block, so an identifier before it
    /// is not a record literal.
    no_struct_literal: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, errors: Vec::new(), in_block: false, no_struct_literal: false }
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

    /// The source offset a node starting at the current token begins at.
    /// Spans are byte offsets into the source, so a diagnostic can quote the
    /// text it is about.
    fn span_start(&self) -> usize {
        self.peek().map(|t| t.span.start).unwrap_or_else(|| self.span_end())
    }

    /// The source offset just past the last token consumed.
    fn span_end(&self) -> usize {
        self.pos.checked_sub(1).and_then(|i| self.tokens.get(i)).map(|t| t.span.end).unwrap_or(0)
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
        let start = self.span_start();
        match self.peek_kind() {
            Some(TokenKind::Data) => self.parse_data(),
            Some(TokenKind::Enum) => self.parse_enum(),
            Some(TokenKind::Menu) => self.parse_menu(),
            Some(TokenKind::Form) => self.parse_form(),
            Some(TokenKind::Plus) | Some(TokenKind::Minus)
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Fn) =>
            {
                let kind = match self.next() {
                    Some(token) => token.kind,
                    None => unreachable!("peek matched a token"),
                };
                Err(ParseError {
                    message: format!(
                        "the `{}` `fn` prefix is no longer supported; use `->` for a positive function or `<-` for a negative function",
                        if matches!(kind, TokenKind::Plus) { "+" } else { "-" }
                    ),
                    span: Span { start, end: self.span_end() },
                })
            }
            Some(TokenKind::Fn) => {
                // fn( — lambda expression, not declaration
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::LParen) {
                    let e = self.parse_expr()?;
                    Ok(Node {
                        span: e.span,
                        kind: Decl::Fn {
                            name: "main".into(),
                            type_params: vec![],
                            bounds: vec![],
                            polarity: FunctionPolarity::Positive,
                            params: vec![],
                            return_type: None,
                            effects: vec![],
                            body: e,
                        },
                    })
                } else {
                    self.parse_fn()
                }
            }
            Some(TokenKind::Command) => self.parse_command_decl(),
            // `mu name(…)` used to be the declaration. It is now the
            // expression that captures the current continuation, and a
            // declaration that abstracts over one is a `command`.
            Some(TokenKind::Mu)
                if matches!(
                    self.tokens.get(self.pos + 1).map(|t| &t.kind),
                    Some(TokenKind::Ident(_))
                ) && matches!(
                    self.tokens.get(self.pos + 2).map(|t| &t.kind),
                    Some(TokenKind::LParen) | Some(TokenKind::Pipe)
                ) =>
            {
                Err(ParseError {
                    message: "a declaration is a `command`; `mu` captures the current continuation"
                        .into(),
                    span: self.peek().map(|t| t.span).unwrap_or(Span { start, end: start }),
                })
            }
            Some(TokenKind::Const) => self.parse_const_decl(),
            Some(TokenKind::Mod) => self.parse_mod_decl(),
            Some(TokenKind::Use) => self.parse_use_decl(),
            Some(TokenKind::Trait) => self.parse_trait_decl(),
            Some(TokenKind::Impl) => self.parse_impl_decl(),
            Some(TokenKind::Effect) => self.parse_effect_decl(),
            _ => {
                // Expression as top-level (for scripting)
                let e = self.parse_expr()?;
                let end = self.span_end();
                let _ = self.eat(&TokenKind::Semicolon);
                Ok(Node {
                    span: Span { start, end },
                    kind: Decl::Fn {
                        name: "main".into(),
                        type_params: vec![],
                        bounds: vec![],
                        polarity: FunctionPolarity::Positive,
                        params: vec![],
                        return_type: None,
                        effects: vec![],
                        body: e,
                    },
                })
            }
        }
    }

    fn parse_data(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Data, "`data`")?;
        let name = self.expect_name("data name")?;
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
        Ok(Node { span: t.span, kind: Decl::Data { name, fields } })
    }

    fn parse_form(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Form, "`form`")?;
        let name = self.expect_name("form name")?;
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
        Ok(Node { span: t.span, kind: Decl::Form { name, fields } })
    }

    fn parse_menu(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Menu, "`menu`")?;
        let name = self.expect_ident("menu name")?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut items = Vec::new();
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let item = self.expect_ident("item name")?;
            self.expect(TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            items.push((item, ty.kind));
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RBrace, "`}`")?;
                break;
            }
        }
        Ok(Node { span: t.span, kind: Decl::Menu { name, items } })
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

    /// An optional effect row: `/ { E1, E2 }`, or nothing for pure.
    fn parse_effect_row(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.eat(&TokenKind::Slash) {
            return Ok(Vec::new());
        }
        self.expect(TokenKind::LBrace, "`{` after `/` in an effect row")?;
        let mut effects = Vec::new();
        if self.eat(&TokenKind::RBrace) {
            return Ok(effects);
        }
        loop {
            effects.push(self.expect_ident("an effect name")?);
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RBrace, "`}` after the effect row")?;
                break;
            }
        }
        Ok(effects)
    }

    fn parse_fn(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Fn, "`fn`")?;
        let name = self.expect_ident("function name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        let params = self.parse_params()?;
        let (polarity, return_type) = self.parse_fn_arrow()?;
        let effects = self.parse_effect_row()?;
        let params = match polarity {
            FunctionPolarity::Positive => params,
            FunctionPolarity::Negative => params
                .into_iter()
                .map(|mut p| {
                    p.is_continuation = true;
                    p
                })
                .collect(),
        };
        let body = self.parse_block()?;
        Ok(Node {
            span: t.span,
            kind: Decl::Fn {
                name,
                type_params,
                bounds,
                polarity,
                params,
                return_type,
                effects,
                body,
            },
        })
    }

    /// Parse a function's required polarity arrow and return type.
    fn parse_fn_arrow(&mut self) -> Result<(FunctionPolarity, Option<TypeExpr>), ParseError> {
        let span = self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
        match self.peek_kind().cloned() {
            Some(TokenKind::Arrow) => {
                self.pos += 1;
                Ok((FunctionPolarity::Positive, Some(self.parse_type()?.kind)))
            }
            Some(TokenKind::ReverseArrow) => {
                self.pos += 1;
                Ok((FunctionPolarity::Negative, Some(self.parse_type()?.kind)))
            }
            _ => Err(ParseError {
                message: "expected `->` for a positive function or `<-` for a negative function"
                    .into(),
                span,
            }),
        }
    }

    /// Type parameters with their bounds: `<T: Show, U>` yields `["T", "U"]`
    /// and `[("T", "Show")]`.
    fn parse_type_params_bounded(&mut self) -> Result<TypeParams, ParseError> {
        let mut params = Vec::new();
        let mut bounds = Vec::new();
        if !self.eat(&TokenKind::Lt) {
            return Ok((params, bounds));
        }
        loop {
            if self.eat(&TokenKind::Gt) {
                break;
            }
            let name = self.expect_ident("type parameter")?;
            // `T: Show` — one bound today; `T: Show + Ord` is deferred.
            while self.eat(&TokenKind::Colon) {
                let trait_name = self.expect_ident("a trait bound")?;
                bounds.push((name.clone(), trait_name));
            }
            params.push(name);
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::Gt, "`>` after type parameters")?;
                break;
            }
        }
        Ok((params, bounds))
    }

    fn parse_command_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Command, "`command`")?;
        let name = self.expect_ident("`command` name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        let (value_params, continuation_params) =
            self.parse_command_params(TypeAnnotations::Required)?;
        let return_type =
            if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?.kind) } else { None };
        if let Some(ref ty) = return_type
            && !matches!(ty, TypeExpr::Bottom)
        {
            return Err(ParseError {
                message: "a `command` returns `⊥`; remove the arrow or annotate `-> ⊥`".into(),
                span: t.span,
            });
        }
        let effects = self.parse_effect_row()?;
        let body = self.parse_block()?;
        Ok(Node {
            span: t.span,
            kind: Decl::Command {
                name,
                type_params,
                bounds,
                value_params,
                continuation_params,
                return_type,
                effects,
                body,
            },
        })
    }

    /// The parameter groups of a `command`: `(values) | (continuations)`, with
    /// either side left out when it has none. `mu f(k)` takes no values,
    /// `mu f(x)` takes no continuations, and an empty group is not written.
    fn parse_command_params(
        &mut self,
        annotations: TypeAnnotations,
    ) -> Result<(Vec<Param>, Vec<Param>), ParseError> {
        let value_params = match self.peek_kind() {
            Some(TokenKind::LParen) => self.parse_group(annotations, "value")?,
            _ => Vec::new(),
        };
        if !self.eat(&TokenKind::Pipe) {
            return Ok((value_params, Vec::new()));
        }
        // A parameter in the second group is a continuation parameter because
        // of where it is declared, not because of anything that follows it.
        let continuation_params = self
            .parse_group(annotations, "continuation")?
            .into_iter()
            .map(|mut p| {
                p.is_continuation = true;
                p
            })
            .collect();
        Ok((value_params, continuation_params))
    }

    /// One group, which must hold something: an empty group is written by
    /// leaving it out.
    fn parse_group(
        &mut self,
        annotations: TypeAnnotations,
        which: &str,
    ) -> Result<Vec<Param>, ParseError> {
        let start = self.span_start();
        let params = self.parse_params_with(annotations)?;
        if params.is_empty() {
            return Err(ParseError {
                message: format!("a group with no {which} parameters is not written"),
                span: Span { start, end: self.span_end() },
            });
        }
        Ok(params)
    }

    fn parse_mod_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Mod, "`mod`")?;
        let name = self.expect_ident("module name")?;
        self.expect(TokenKind::LBrace, "`{` after the module name")?;
        let mut decls = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(ParseError {
                    message: format!("module `{name}` is missing its closing `}}`"),
                    span: t.span,
                });
            }
            decls.push(self.parse_decl()?);
        }
        Ok(Node { span: t.span, kind: Decl::Mod { name, decls } })
    }

    fn parse_use_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Use, "`use`")?;
        let mut path = vec![self.expect_ident("a path to use")?];
        while self.eat(&TokenKind::ColonColon) {
            path.push(self.expect_ident("a path segment")?);
        }
        if path.len() < 2 {
            return Err(ParseError {
                message: "`use` takes a path with at least two segments, `module::name`".into(),
                span: t.span,
            });
        }
        let _ = self.eat(&TokenKind::Semicolon);
        Ok(Node { span: t.span, kind: Decl::Use { path } })
    }

    fn parse_effect_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Effect, "`effect`")?;
        let name = self.expect_ident("effect name")?;
        self.expect(TokenKind::LBrace, "`{` after the effect name")?;
        let mut operations = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(ParseError {
                    message: format!("effect `{name}` is missing its closing `}}`"),
                    span: t.span,
                });
            }
            self.expect(TokenKind::Fn, "`fn` for an operation")?;
            let op = self.expect_ident("operation name")?;
            let params = self.parse_params()?;
            let return_type =
                if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?.kind) } else { None };
            self.expect(TokenKind::Semicolon, "`;` after an operation")?;
            operations.push(EffectOp { name: op, params, return_type });
        }
        Ok(Node { span: t.span, kind: Decl::Effect { name, operations } })
    }

    fn parse_trait_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Trait, "`trait`")?;
        let name = self.expect_ident("trait name")?;
        self.expect(TokenKind::LBrace, "`{` after the trait name")?;
        let mut methods = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(ParseError {
                    message: format!("trait `{name}` is missing its closing `}}`"),
                    span: t.span,
                });
            }
            methods.push(self.parse_trait_method()?);
        }
        Ok(Node { span: t.span, kind: Decl::Trait { name, methods } })
    }

    /// A method signature: a `fn` or `command` header ending in `;`.
    fn parse_trait_method(&mut self) -> Result<TraitMethod, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Fn) => {
                self.pos += 1;
                let name = self.expect_ident("method name")?;
                let params = self.parse_params()?;
                let (polarity, return_type) = self.parse_fn_arrow()?;
                let value_params = match polarity {
                    FunctionPolarity::Positive => params,
                    FunctionPolarity::Negative => params
                        .into_iter()
                        .map(|mut p| {
                            p.is_continuation = true;
                            p
                        })
                        .collect(),
                };
                self.expect(TokenKind::Semicolon, "`;` after a method signature")?;
                Ok(TraitMethod {
                    name,
                    is_command: false,
                    polarity,
                    value_params,
                    continuation_params: Vec::new(),
                    return_type,
                })
            }
            Some(TokenKind::Command) => {
                self.pos += 1;
                let name = self.expect_ident("method name")?;
                let (value_params, continuation_params) =
                    self.parse_command_params(TypeAnnotations::Required)?;
                self.expect(TokenKind::Semicolon, "`;` after a method signature")?;
                Ok(TraitMethod {
                    name,
                    is_command: true,
                    polarity: FunctionPolarity::Positive,
                    value_params,
                    continuation_params,
                    return_type: None,
                })
            }
            _ => Err(ParseError {
                message: "a trait method is a `fn` or `command` signature".into(),
                span: self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 }),
            }),
        }
    }

    fn parse_impl_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Impl, "`impl`")?;
        // impl<...> bounds are parsed and kept on the methods, not the header,
        // in v1: a generic impl's methods carry the bound.
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        let trait_name = self.expect_ident("a trait name")?;
        self.expect(TokenKind::For, "`for` in an `impl`")?;
        let for_type = self.parse_type()?.kind;
        self.expect(TokenKind::LBrace, "`{` after the impl header")?;
        let mut methods = Vec::new();
        while !self.eat(&TokenKind::RBrace) {
            if self.peek().is_none() {
                return Err(ParseError {
                    message: format!("`impl {trait_name}` is missing its closing `}}`"),
                    span: t.span,
                });
            }
            methods.push(self.parse_decl()?);
        }
        Ok(Node {
            span: t.span,
            kind: Decl::Impl { trait_name, type_params, bounds, for_type, methods },
        })
    }

    fn parse_const_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Const, "`const`")?;
        let name = self.expect_ident("constant name")?;
        self.expect(TokenKind::Colon, "`:` in constant declaration")?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Assign, "`=` in constant declaration")?;
        let value = self.parse_expr()?;
        let end = self.span_end();
        self.eat(&TokenKind::Semicolon);
        Ok(Node {
            span: Span { start: t.span.start, end },
            kind: Decl::Const { name, ty: ty.kind, value },
        })
    }

    /// A declaration's parameters are its interface, so their types are
    /// written. A local `mu` is not an interface: its types may be left to
    /// the body that uses them.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.parse_params_with(TypeAnnotations::Required)
    }

    fn parse_params_with(
        &mut self,
        annotations: TypeAnnotations,
    ) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        self.expect(TokenKind::LParen, "`(`")?;
        loop {
            if self.eat(&TokenKind::RParen) {
                break;
            }
            let name = self.expect_ident("parameter name")?;
            let ty = if annotations == TypeAnnotations::Required {
                self.expect(TokenKind::Colon, "`:` — a declaration's parameters carry types")?;
                Some(self.parse_type()?.kind)
            } else if self.eat(&TokenKind::Colon) {
                Some(self.parse_type()?.kind)
            } else {
                None
            };
            params.push(Param { name, ty, is_continuation: false });
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RParen, "`)`")?;
                break;
            }
        }
        Ok(params)
    }

    fn parse_data_expr_fields(&mut self) -> Result<Vec<(String, Node<Expr>)>, ParseError> {
        let mut fields = Vec::new();
        loop {
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let field = self.expect_ident("record field name")?;
            self.expect(TokenKind::Colon, "`:` in record literal")?;
            let value = self.parse_expr()?;
            fields.push((field, value));
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RBrace, "`}` after record literal")?;
                break;
            }
        }
        Ok(fields)
    }

    fn parse_block(&mut self) -> Result<Node<Expr>, ParseError> {
        let start = self.span_start();
        self.expect(TokenKind::LBrace, "`{`")?;
        let outer_in_block = self.in_block;
        let outer_no_struct_literal = self.no_struct_literal;
        self.in_block = true;
        self.no_struct_literal = false;
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
        // An empty block produces unit, like `()`.
        let mut kind = if exprs.is_empty() { Expr::Pair(Vec::new()) } else { Expr::Block(exprs) };
        // Wrap from the inside out: each trailing Let captures the rest.
        // (Handled during lowering: Block flattens Lets by nesting.)
        let _ = &mut kind;
        self.in_block = outer_in_block;
        self.no_struct_literal = outer_no_struct_literal;
        Ok(Node { span: Span { start, end: self.span_end() }, kind })
    }

    fn expect_name(&mut self, what: &str) -> Result<String, ParseError> {
        match self.peek_kind().cloned() {
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                Ok(s)
            }
            Some(TokenKind::Return) => {
                self.pos += 1;
                Ok("return".to_string())
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

    fn expect_ident(&mut self, what: &str) -> Result<String, ParseError> {
        self.expect_name(what)
    }

    pub fn parse_type(&mut self) -> Result<Node<TypeExpr>, ParseError> {
        let start = self.span_start();
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
            Some(TokenKind::Down) => {
                self.pos += 1;
                TypeExpr::Down(Box::new(self.parse_type()?))
            }
            Some(TokenKind::Up) => {
                self.pos += 1;
                TypeExpr::Up(Box::new(self.parse_type()?))
            }
            Some(TokenKind::LParen) => {
                self.pos += 1;
                let left = self.parse_type()?;
                if self.eat(&TokenKind::Arrow) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Fun(
                        Box::new(left),
                        Box::new(Node {
                            span: Span { start, end: self.span_end() },
                            kind: right.kind,
                        }),
                    )
                } else if self.eat(&TokenKind::Tensor) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Tensor(
                        Box::new(left),
                        Box::new(Node {
                            span: Span { start, end: self.span_end() },
                            kind: right.kind,
                        }),
                    )
                } else if self.eat(&TokenKind::Par) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Par(
                        Box::new(left),
                        Box::new(Node {
                            span: Span { start, end: self.span_end() },
                            kind: right.kind,
                        }),
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
                let mut s = s;
                while self.peek_kind() == Some(&TokenKind::ColonColon) {
                    self.pos += 1;
                    let segment = self.expect_ident("a path segment")?;
                    s = format!("{s}::{segment}");
                }
                TypeExpr::Base(s)
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
        Ok(Node { span: Span { start, end: self.span_end() }, kind })
    }

    pub fn parse_expr(&mut self) -> Result<Node<Expr>, ParseError> {
        // A cut binds more loosely than every operator: `a + b @ k` sends the
        // sum to `k`. It is not associative — a cut has no result, so it
        // cannot be the value of another cut.
        let value = self.parse_binary(0)?;
        if !self.eat(&TokenKind::At) {
            return Ok(value);
        }
        let consumer = self.parse_binary(0)?;
        if self.peek_kind() == Some(&TokenKind::At) {
            return Err(ParseError {
                message: "a cut has no result, so it cannot be cut again; \
                          write one `@` per command"
                    .into(),
                span: Span { start: value.span.start, end: self.span_end() },
            });
        }
        Ok(Node {
            span: Span { start: value.span.start, end: consumer.span.end },
            kind: Expr::Cut { value: Box::new(value), consumer: Box::new(consumer) },
        })
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Node<Expr>, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let (op, prec) = match self.peek_kind() {
                Some(TokenKind::Plus) => (BinOp::Add, 3),
                Some(TokenKind::Minus) => (BinOp::Sub, 3),
                Some(TokenKind::Star) | Some(TokenKind::Tensor) => (BinOp::Mul, 4),
                Some(TokenKind::Slash) => (BinOp::Div, 4),
                Some(TokenKind::Percent) => (BinOp::Mod, 4),
                Some(TokenKind::EqEq) => (BinOp::Eq, 2),
                Some(TokenKind::NotEq) => (BinOp::Ne, 2),
                Some(TokenKind::Lt) => (BinOp::Lt, 2),
                Some(TokenKind::Gt) => (BinOp::Gt, 2),
                Some(TokenKind::Le) => (BinOp::Le, 2),
                Some(TokenKind::Ge) => (BinOp::Ge, 2),
                Some(TokenKind::AmpAmp) => (BinOp::And, 1),
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
        if self.peek_kind() == Some(&TokenKind::Bang) {
            let start = self.span_start();
            self.pos += 1;
            let body = self.parse_unary()?;
            let end = self.span_end();
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::UnOp { op: UnOp::Not, body: Box::new(body) },
            });
        }
        if self.peek_kind() == Some(&TokenKind::Minus) {
            let start = self.span_start();
            self.pos += 1;
            let body = self.parse_unary()?;
            let end = self.span_end();
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::UnOp { op: UnOp::Neg, body: Box::new(body) },
            });
        }
        // `↓e` boxes a consumer as data, `↑e` opens the box. They bind as
        // tightly as the other prefixes, and they are written the way their
        // types are.
        if matches!(self.peek_kind(), Some(TokenKind::Down) | Some(TokenKind::Up)) {
            let start = self.span_start();
            let down = self.peek_kind() == Some(&TokenKind::Down);
            self.pos += 1;
            let expr = self.parse_unary()?;
            let end = self.span_end();
            return Ok(Node {
                span: Span { start, end },
                kind: Expr::Shift { down, expr: Box::new(expr) },
            });
        }
        self.parse_postfix()
    }

    /// Parse an expression in a position where a following `{` opens a block
    /// rather than a record literal: the condition of an `if`, the scrutinee
    /// of a `match`.
    fn parse_scrutinee(&mut self) -> Result<Node<Expr>, ParseError> {
        let outer = self.no_struct_literal;
        self.no_struct_literal = true;
        let parsed = self.parse_expr();
        self.no_struct_literal = outer;
        parsed
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
                    let end = self.span_end();
                    e = Node {
                        span: Span { start: e.span.start, end },
                        kind: Expr::Call { callee: Box::new(e), args },
                    };
                }
                Some(TokenKind::Dot) => {
                    self.pos += 1;
                    if self.peek_kind() == Some(&TokenKind::ColonColon) {
                        self.pos += 1;
                        let variant = self.expect_ident("enum variant name")?;
                        if let Expr::Ident(name) = &e.kind {
                            e = Node {
                                span: e.span,
                                kind: Expr::Ident(format!("{name}::{variant}")),
                            };
                            continue;
                        }
                        return Err(ParseError {
                            message: "enum variant paths require an enum name".into(),
                            span: e.span,
                        });
                    }
                    let field_name = self.peek_kind().cloned();
                    if matches!(field_name, Some(TokenKind::Ident(_)) | Some(TokenKind::Return))
                        && self.tokens.get(self.pos + 1).map(|t| &t.kind)
                            == Some(&TokenKind::LBrace)
                    {
                        let name = self.expect_name("data name")?;
                        let fields = self.parse_data_expr_fields()?;
                        let end = self.span_end();
                        e = Node {
                            span: Span { start: e.span.start, end },
                            kind: Expr::Data { name, fields },
                        };
                        continue;
                    }
                    // `base.0` — positional projection of a tuple.
                    if let Some(TokenKind::Int(n)) = field_name {
                        self.pos += 1;
                        let end = self.span_end();
                        let start = e.span.start;
                        e = Node {
                            span: Span { start, end },
                            kind: Expr::Project {
                                base: Box::new(e),
                                key: ProjKey::Index(n as usize),
                            },
                        };
                        continue;
                    }
                    // `base.field` — projection of a record field by name.
                    if matches!(field_name, Some(TokenKind::Ident(_)) | Some(TokenKind::Return)) {
                        let name = self.expect_name("field name")?;
                        let end = self.span_end();
                        let start = e.span.start;
                        e = Node {
                            span: Span { start, end },
                            kind: Expr::Project { base: Box::new(e), key: ProjKey::Field(name) },
                        };
                        continue;
                    }
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
                    let end = self.span_end();
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
        let start = self.span_start();
        match self.peek_kind().cloned() {
            // `.item(k)` — a request literal: one demand on a menu, carrying
            // the continuation that wants the answer.
            Some(TokenKind::Dot) => {
                self.pos += 1;
                let dtor = self.expect_ident("destructor name")?;
                self.expect(TokenKind::LParen, "`(` after the destructor")?;
                let arg = self.parse_expr()?;
                self.expect(TokenKind::RParen, "`)`")?;
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Request { dtor, arg: Box::new(arg) },
                })
            }
            Some(TokenKind::Int(n)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Int(n) })
            }
            Some(TokenKind::Float(n)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Float(n) })
            }
            Some(TokenKind::Str(s)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Str(s) })
            }
            Some(TokenKind::Char(c)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Char(c) })
            }
            Some(TokenKind::Bool(b)) => {
                self.pos += 1;
                Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Bool(b) })
            }
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                let mut s = s;
                while self.peek_kind() == Some(&TokenKind::ColonColon) {
                    self.pos += 1;
                    let segment = self.expect_ident("a path segment")?;
                    s = format!("{s}::{segment}");
                }
                // `S { field: value }` is a record literal wherever a `{`
                // here cannot be a block.
                if !self.no_struct_literal && self.peek_kind() == Some(&TokenKind::LBrace) {
                    self.pos += 1;
                    let fields = self.parse_data_expr_fields()?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::Data { name: s, fields },
                    });
                }
                Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Ident(s) })
            }
            Some(TokenKind::Return) => {
                self.pos += 1;
                if !self.no_struct_literal && self.peek_kind() == Some(&TokenKind::LBrace) {
                    self.pos += 1;
                    let fields = self.parse_data_expr_fields()?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::Data { name: "return".into(), fields },
                    });
                }
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Ident("return".into()),
                })
            }
            Some(TokenKind::Mu) => {
                self.pos += 1;
                // Nothing refers to a local `mu`'s name, so it is optional.
                let name = match self.peek_kind() {
                    Some(TokenKind::Ident(_)) => Some(self.expect_ident("local `mu` name")?),
                    _ => None,
                };
                // One group, and it is the continuation the expression
                // captures — `fn(x)` binds a value, `mu(k)` binds the
                // continuation. Nothing separates it from a second group,
                // because a `mu` has no second group.
                if self.peek_kind() == Some(&TokenKind::Pipe) {
                    return Err(ParseError {
                        message: "a `mu` binds only the continuation it captures: write `mu(k)`"
                            .into(),
                        span: self.peek().map(|t| t.span).unwrap_or(Span { start, end: start }),
                    });
                }
                let continuation_params: Vec<Param> = self
                    .parse_group(TypeAnnotations::Optional, "continuation")?
                    .into_iter()
                    .map(|mut p| {
                        p.is_continuation = true;
                        p
                    })
                    .collect();
                if self.peek_kind() == Some(&TokenKind::Pipe) {
                    return Err(ParseError {
                        message: "a `mu` has one parameter group: the continuation it captures"
                            .into(),
                        span: self.peek().map(|t| t.span).unwrap_or(Span { start, end: start }),
                    });
                }
                let body = self.parse_block()?;
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Mu { name, continuation_params, body: Box::new(body) },
                })
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
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Lambda { param, param_type, return_type, body: Box::new(body) },
                })
            }
            Some(TokenKind::Handle) => {
                self.pos += 1;
                let body = self.parse_scrutinee()?;
                self.expect(TokenKind::LBrace, "`{` after the handled expression")?;
                let mut clauses = Vec::new();
                let mut ret = None;
                loop {
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    // `return(x) => body` or `op(params) resume => body`.
                    if self.peek_kind() == Some(&TokenKind::Return) {
                        self.pos += 1;
                        self.expect(TokenKind::LParen, "`(` after `return`")?;
                        let binder = self.expect_ident("the return binder")?;
                        self.expect(TokenKind::RParen, "`)`")?;
                        self.expect(TokenKind::FatArrow, "`=>` in a return clause")?;
                        ret = Some((binder, Box::new(self.parse_expr()?)));
                    } else {
                        let op = self.expect_ident("an operation name")?;
                        self.expect(TokenKind::LParen, "`(` after the operation")?;
                        let mut params = Vec::new();
                        if !self.eat(&TokenKind::RParen) {
                            loop {
                                params.push(self.expect_ident("an operation parameter")?);
                                if !self.eat(&TokenKind::Comma) {
                                    self.expect(TokenKind::RParen, "`)`")?;
                                    break;
                                }
                            }
                        }
                        let resume = self.expect_ident("the resume binder")?;
                        self.expect(TokenKind::FatArrow, "`=>` in a handler clause")?;
                        let body = self.parse_expr()?;
                        clauses.push(HandleClause { op, params, resume, body });
                    }
                    self.eat(&TokenKind::Comma);
                }
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Handle { body: Box::new(body), clauses, ret },
                })
            }
            Some(TokenKind::Match) => {
                self.pos += 1;
                let scrutinee = self.parse_scrutinee()?;
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
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Match { scrutinee: Box::new(scrutinee), arms },
                })
            }
            Some(TokenKind::Select) => {
                self.pos += 1;
                // The type whose consumer this builds: a declaration name, or
                // an explicit connective such as `(+i64 ⊗ +String)`. It may be
                // left out when an arm's pattern names it.
                let ty = match self.peek_kind() {
                    Some(TokenKind::LBrace) => None,
                    _ => Some(Box::new(self.parse_type()?)),
                };
                self.expect(TokenKind::LBrace, "`{` after the `select` type")?;
                let mut arms = Vec::new();
                loop {
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    // An arm reads against the flow of a `match` arm: the
                    // shape that arrives is on the left, and `<=` points back
                    // at the command it runs.
                    let arm = self.pos;
                    let pattern = match self.parse_pattern() {
                        Ok(pattern) => pattern,
                        Err(e) => return Err(self.reversed_arm_error(arm, e)),
                    };
                    if !self.eat(&TokenKind::Le) {
                        let expected =
                            self.expect(TokenKind::Le, "`<=` in `select` arm").unwrap_err();
                        return Err(self.reversed_arm_error(arm, expected));
                    }
                    let command = self.parse_expr()?;
                    arms.push(SelectArm { pattern, command });
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(TokenKind::RBrace, "`}` after `select` arm")?;
                        break;
                    }
                }
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Select { ty, arms },
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
                        span: Span { start, end: self.span_end() },
                    });
                };
                let body = if !self.in_block && self.eat(&TokenKind::Semicolon) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::Let { name, ty, value: Box::new(value), body },
                })
            }
            Some(TokenKind::If) => {
                self.pos += 1;
                let cond = self.parse_scrutinee()?;
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
                    span: Span { start, end: self.span_end() },
                    kind: Expr::If { cond: Box::new(cond), then: Box::new(then), otherwise },
                })
            }
            Some(TokenKind::LParen) => {
                self.pos += 1;
                // () or (e) or (e1, e2)
                if self.eat(&TokenKind::RParen) {
                    // `()` is the unit value: the empty product.
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::Pair(Vec::new()),
                    });
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
                    Ok(Node { span: Span { start, end: self.span_end() }, kind: Expr::Pair(items) })
                } else {
                    self.expect(TokenKind::RParen, "`)`")?;
                    Ok(Node { span: Span { start, end: self.span_end() }, kind: first.kind })
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

    /// Explain the old arm order rather than reporting where its command
    /// failed to parse as a pattern. An arm holding a `=>` before it ends was
    /// written the other way round; the rest of the `select` is then skipped,
    /// so one arm in the old order reports one error.
    fn reversed_arm_error(&mut self, arm: usize, fallback: ParseError) -> ParseError {
        let mut depth = 0i32;
        for token in &self.tokens[arm..] {
            match token.kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket if depth > 0 => {
                    depth -= 1;
                }
                // The `}` that closes the arm list, or the `,` that ends this
                // arm: either way the arm is over.
                TokenKind::RBrace | TokenKind::Comma => break,
                TokenKind::FatArrow if depth == 0 => {
                    let span = token.span;
                    self.skip_past_arm_list(arm);
                    return ParseError {
                        message: "a `select` arm is written `pattern <= command`: the shape comes first, as in a `match`".into(),
                        span,
                    };
                }
                _ => {}
            }
        }
        fallback
    }

    /// Move past the `}` that closes the arm list an arm belongs to, so a
    /// diagnostic about one arm does not cascade into the arms after it.
    fn skip_past_arm_list(&mut self, arm: usize) {
        let mut depth = 0i32;
        for (offset, token) in self.tokens[arm..].iter().enumerate() {
            match token.kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace if depth == 0 => {
                    self.pos = arm + offset + 1;
                    return;
                }
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => depth -= 1,
                _ => {}
            }
        }
        self.pos = self.tokens.len();
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
            // `.item(out)` — a request shape: the demanded destructor, and
            // the binder naming the request's continuation.
            Some(TokenKind::Dot) => {
                self.pos += 1;
                let dtor = self.expect_ident("destructor name")?;
                self.expect(TokenKind::LParen, "`(` after the destructor")?;
                let binder = self.expect_ident("a binder for the request's continuation")?;
                self.expect(TokenKind::RParen, "`)`")?;
                Ok(Pattern::Dtor { dtor, binder })
            }
            Some(TokenKind::Minus) => {
                self.pos += 1;
                match self.parse_single_pattern()? {
                    Pattern::Int(n) => Ok(Pattern::Int(-n)),
                    Pattern::Float(n) => Ok(Pattern::Float(-n)),
                    _ => Err(ParseError {
                        message: "negative patterns require a numeric literal".into(),
                        span: Span { start: self.pos - 1, end: self.span_end() },
                    }),
                }
            }
            Some(TokenKind::Ident(s)) => {
                self.pos += 1;
                if s == "_" {
                    return Ok(Pattern::Wildcard);
                }
                // Path pattern: Enum::Variant or Enum::Variant(fields),
                // with the enum itself possibly module-qualified — or a
                // qualified data name, when a `{` follows the path.
                let mut s = s;
                if self.peek_kind() == Some(&TokenKind::ColonColon) {
                    let mut segments = vec![s.clone()];
                    while self.eat(&TokenKind::ColonColon) {
                        segments.push(self.expect_ident("a path segment")?);
                    }
                    if self.peek_kind() == Some(&TokenKind::LBrace) {
                        s = segments.join("::");
                    } else {
                        let variant = segments.pop().expect("at least two segments");
                        let s = segments.join("::");
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
                }
                // Struct pattern with field shorthand: Point { x, y: pat }
                if self.peek_kind() == Some(&TokenKind::LBrace) {
                    self.pos += 1;
                    let mut fields = Vec::new();
                    loop {
                        if self.eat(&TokenKind::RBrace) {
                            break;
                        }
                        let field = self.expect_ident("record field name")?;
                        let pattern = if self.eat(&TokenKind::Colon) {
                            self.parse_pattern()?
                        } else {
                            Pattern::Ident(field.clone())
                        };
                        fields.push((field, pattern));
                        if !self.eat(&TokenKind::Comma) {
                            self.expect(TokenKind::RBrace, "`}` after record pattern")?;
                            break;
                        }
                    }
                    return Ok(Pattern::Data { name: s, fields });
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
            Some(TokenKind::LBracket) => {
                self.pos += 1;
                let mut items = Vec::new();
                let mut rest = None;
                loop {
                    if self.eat(&TokenKind::RBracket) {
                        break;
                    }
                    if self.peek_kind() == Some(&TokenKind::DotDot) {
                        self.pos += 1;
                        rest = Some(Box::new(self.parse_pattern()?));
                        self.expect(TokenKind::RBracket, "`]` after list rest pattern")?;
                        break;
                    }
                    items.push(self.parse_pattern()?);
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(TokenKind::RBracket, "`]` after list pattern")?;
                        break;
                    }
                }
                Ok(Pattern::List { items, rest })
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
    fn parse_negative_fn_def() {
        let p = parse_str("fn run(k: -i32) <- i32 { k(1) }");
        assert_eq!(p.decls.len(), 1);
        assert!(matches!(
            &p.decls[0].kind,
            Decl::Fn { name, polarity, params, .. }
                if name == "run"
                    && *polarity == FunctionPolarity::Negative
                    && params.iter().all(|p| p.is_continuation)
        ));
    }

    #[test]
    fn parse_fn_polarity_comes_from_arrow_not_lookahead() {
        let p = parse_str("fn run(k: -i32) <- i32 { k(1) }");
        assert!(matches!(
            &p.decls[0].kind,
            Decl::Fn { polarity, params, .. }
                if *polarity == FunctionPolarity::Negative
                    && params.iter().all(|p| p.is_continuation)
        ));

        let p = parse_str("fn run(k: -i32) -> i32 { k(1) }");
        assert!(matches!(
            &p.decls[0].kind,
            Decl::Fn { polarity, params, .. }
                if *polarity == FunctionPolarity::Positive
                    && params.iter().all(|p| !p.is_continuation)
        ));
    }

    #[test]
    fn parse_return_in_ordinary_positions() {
        // `return` may be declared as a continuation parameter...
        let p = parse_str("fn k(return: -i32) <- i32 { return(0) }");
        assert!(matches!(
            &p.decls[0].kind,
            Decl::Fn { params, .. } if params.first().is_some_and(|p| p.name == "return")
        ));

        // ...and used as an expression callee and an expression argument.
        let p = parse_str("fn main() -> i32 { return(0, return) }");
        let body = match &p.decls[0].kind {
            Decl::Fn { body, .. } => body.clone(),
            _ => panic!("expected fn declaration"),
        };
        assert!(matches!(
            &body.kind,
            Expr::Block(exprs)
                if matches!(&exprs[0].kind,
                    Expr::Call { callee, args }
                        if matches!(&callee.kind, Expr::Ident(name) if name == "return")
                            && matches!(&args[0].kind, Expr::Int(n) if *n == 0)
                            && matches!(&args[1].kind, Expr::Ident(name) if name == "return")
                )
        ));

        // In ordinary call syntax it can also be a struct-name marker.
        let p = parse_str("fn main() -> i32 { return(return { x: 0 }) }");
        let body = match &p.decls[0].kind {
            Decl::Fn { body, .. } => body.clone(),
            _ => panic!("expected fn declaration"),
        };
        assert!(matches!(
            &body.kind,
            Expr::Block(exprs)
                if matches!(&exprs[0].kind,
                    Expr::Call { callee, args }
                        if matches!(&callee.kind, Expr::Ident(name) if name == "return")
                            && matches!(&args[0].kind,
                                Expr::Data { name, .. } if name == "return")
                )
        ));
    }

    #[test]
    fn parse_rejects_bare_fn() {
        let tokens = lex("fn f(x: +i32) { x }").unwrap();
        let errors = parse(tokens).unwrap_err();
        assert!(
            errors[0]
                .message
                .contains("expected `->` for a positive function or `<-` for a negative function"),
            "got: {errors:?}"
        );
    }

    #[test]
    fn parse_rejects_polarity_prefixed_fn() {
        for source in ["+fn f(x: +i32) -> i32 { x }", "-fn f(k: -i32) <- i32 { k(1) }"] {
            let errors = parse(lex(source).unwrap()).unwrap_err();
            assert!(
                errors[0].message.contains("`fn` prefix is no longer supported"),
                "source: {source}; errors: {errors:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_old_to_parameter_marker() {
        let source = "command step(x: +i32, to k: -i32) { k(x) }";
        let errors = parse(lex(source).unwrap()).unwrap_err();
        assert!(errors[0].message.contains("expected `:`"), "source: {source}; errors: {errors:?}");
    }

    #[test]
    fn parse_mu_def() {
        let p = parse_str("command step(x: +i32) | (k: -i32) { k(x) }");
        assert_eq!(p.decls.len(), 1);
        let d = &p.decls[0];
        assert!(matches!(&d.kind, Decl::Command { name, value_params, continuation_params, .. }
                if name == "step" && value_params.len() == 1 && continuation_params.len() == 1));
    }

    #[test]
    fn parse_command_bottom_annotation() {
        let p = parse_str("command step(x: +i32) | (k: -i32) -> ⊥ { k(x) }");
        assert!(matches!(
            &p.decls[0].kind,
            Decl::Command { return_type: Some(TypeExpr::Bottom), .. }
        ));
    }

    #[test]
    fn parse_command_rejects_non_bottom_return() {
        let errors =
            parse(lex("command step(x: +i32) | (k: -i32) -> i32 { k(x) }").unwrap()).unwrap_err();
        assert!(errors[0].message.contains("a `command` returns `⊥`"), "got: {errors:?}");
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
    fn parse_select() {
        // One arm per variant of an enum.
        let p = parse_str("select Color { Red <= 0 @ return, Green <= 1 @ return }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Select { ty, arms } = &body.kind else {
            panic!("expected a select: {:?}", body.kind)
        };
        assert!(
            matches!(ty.as_deref().map(|ty| &ty.kind), Some(TypeExpr::Base(name)) if name == "Color")
        );
        assert_eq!(arms.len(), 2);
        assert!(matches!(&arms[0].pattern, Pattern::Ident(name) if name == "Red"));
    }

    #[test]
    fn a_local_mu_may_leave_out_its_name_and_its_parameter_types() {
        let p = parse_str("mu(k) { 42 @ k }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Mu { name, continuation_params, .. } = &body.kind else {
            panic!("expected a local mu: {:?}", body.kind)
        };
        assert_eq!(*name, None);
        assert_eq!(continuation_params[0].name, "k");
        assert_eq!(continuation_params[0].ty, None);

        // Either may still be written. With a name it needs an enclosing
        // declaration: `mu name(…)` at the top level is a declaration.
        let p = parse_str("fn f() -> i32 { mu here(k: -i32) { 42 @ k } }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected a block: {:?}", body.kind) };
        let Expr::Mu { name, continuation_params, .. } = &exprs[0].kind else {
            panic!("expected a local mu: {:?}", exprs[0].kind)
        };
        assert_eq!(name.as_deref(), Some("here"));
        assert!(continuation_params[0].ty.is_some());
    }

    #[test]
    fn a_declaration_is_a_command_and_mu_is_the_expression() {
        // `mu name(…)` was the declaration before the two forms were told
        // apart; the diagnostic says which is which.
        for source in ["mu main(exit: -i32) { 0 @ exit }", "mu f(x: +i32) | (k: -i32) { x @ k }"] {
            let errors = parse(lex(source).unwrap()).unwrap_err();
            assert!(
                errors.iter().any(|e| e.message.contains("a declaration is a `command`")),
                "{source}: {errors:?}"
            );
        }

        // A named `mu` inside a declaration is still the capturing form.
        let p = parse_str("command f | (k: -i32) { 1 @ k }");
        assert!(matches!(&p.decls[0].kind, Decl::Command { .. }));
        let p = parse_str("fn g() -> i32 { mu here(k: -i32) { 1 @ k } }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a fn") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected a block") };
        assert!(matches!(&exprs[0].kind, Expr::Mu { .. }));
    }

    #[test]
    fn a_mu_binds_one_group_and_it_is_the_continuation() {
        // `fn(x)` binds a value, `mu(k)` binds the continuation it captures.
        let p = parse_str("fn f() -> i32 { mu(k: -i32) { 1 @ k } }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a fn") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected a block") };
        let Expr::Mu { continuation_params, .. } = &exprs[0].kind else {
            panic!("expected a mu: {:?}", exprs[0].kind)
        };
        assert_eq!(continuation_params[0].name, "k");
        assert!(continuation_params[0].is_continuation);

        // There is no second group to separate, so `|` is a mistake.
        for source in
            ["fn f() -> i32 { mu | (k) { 1 @ k } }", "fn f() -> i32 { mu(x) | (k) { x @ k } }"]
        {
            let errors = parse(lex(source).unwrap()).unwrap_err();
            assert!(
                errors.iter().any(|e| e.message.contains("continuation it captures")),
                "{source}: {errors:?}"
            );
        }
    }

    #[test]
    fn a_mu_writes_only_the_parameter_groups_it_has() {
        // No values: the group is left out, not written empty.
        let p = parse_str("command main | (exit: -i32) { 0 @ exit }");
        let Decl::Command { value_params, continuation_params, .. } = &p.decls[0].kind else {
            panic!("expected a mu declaration: {:?}", p.decls[0].kind)
        };
        assert!(value_params.is_empty());
        assert_eq!(continuation_params[0].name, "exit");
        assert!(continuation_params[0].is_continuation);

        // No continuations: the `|` goes with the group it introduces.
        let p = parse_str("command log(message: +String) { println(message) }");
        let Decl::Command { value_params, continuation_params, .. } = &p.decls[0].kind else {
            panic!("expected a mu declaration: {:?}", p.decls[0].kind)
        };
        assert_eq!(value_params[0].name, "message");
        assert!(continuation_params.is_empty());

        for source in
            ["command main() | (exit: -i32) { 0 @ exit }", "command log(m: +String) | () { m }"]
        {
            let errors = parse(lex(source).unwrap()).unwrap_err();
            assert!(
                errors.iter().any(|e| e.message.contains("is not written")),
                "{source}: {errors:?}"
            );
        }
    }

    #[test]
    fn a_declaration_still_needs_its_parameter_types() {
        let errors = parse(lex("fn f(x) -> i32 { x }").unwrap()).unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("a declaration's parameters carry types")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn a_select_may_leave_out_its_type() {
        let p = parse_str("select { Red <= 0 @ return }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Select { ty, arms } = &body.kind else {
            panic!("expected a select: {:?}", body.kind)
        };
        assert!(ty.is_none());
        assert_eq!(arms.len(), 1);
    }

    #[test]
    fn parse_rejects_the_old_select_arm_order() {
        // `command => pattern` was the order before the arrow was turned
        // around; saying so beats reporting that `0` is not a pattern.
        let errors = parse(
            lex("enum Color { Red, Green }
                 fn k(return: -i32) <- Color {
                     select Color {
                         0 @ return => Red,
                         1 @ return => Green,
                     }
                 }")
            .unwrap(),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("`pattern <= command`")),
            "errors: {errors:?}"
        );
        // One arm in the old order is one error, not one per arm after it.
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
    }

    #[test]
    fn parse_select_over_a_product() {
        // A product has one shape, so one arm, binding its components.
        let p = parse_str("select (+i64 ⊗ +String) { (end, text) <= end @ done }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Select { ty, arms } = &body.kind else {
            panic!("expected a select: {:?}", body.kind)
        };
        assert!(matches!(ty.as_deref().map(|ty| &ty.kind), Some(TypeExpr::Tensor(..))), "{ty:?}");
        assert_eq!(arms.len(), 1);
        assert!(matches!(&arms[0].pattern, Pattern::Tuple(items) if items.len() == 2));
    }

    #[test]
    fn parse_select_over_a_struct() {
        let p = parse_str("select Reading { Reading { value: v, unit: u } <= 0 @ out }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Select { arms, .. } = &body.kind else {
            panic!("expected a select: {:?}", body.kind)
        };
        assert!(
            matches!(&arms[0].pattern, Pattern::Data { name, fields }
                if name == "Reading" && fields.len() == 2),
            "{:?}",
            arms[0].pattern
        );
    }

    #[test]
    fn parse_cut_binds_more_loosely_than_every_operator() {
        // `1 + 2 @ k` sends the sum to `k`.
        let p = parse_str("fn main() -> i32 { 1 + 2 @ k }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected fn") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected block") };
        let Expr::Cut { value, consumer } = &exprs[0].kind else {
            panic!("expected a cut: {:?}", exprs[0].kind)
        };
        assert!(matches!(value.kind, Expr::BinOp { .. }), "value: {:?}", value.kind);
        assert!(matches!(&consumer.kind, Expr::Ident(name) if name == "k"));
    }

    #[test]
    fn parse_rejects_a_cut_of_a_cut() {
        let errors = parse(lex("fn main() -> i32 { 1 @ j @ k }").unwrap()).unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("a cut has no result")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn parse_rejects_removed_expression_level_dual() {
        let errors = parse(lex("fn main() -> i32 { dual(42) }").unwrap()).unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("expected expression, found `dual`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn parse_rejects_removed_command_type_former() {
        let errors = parse(lex("fn f(x: Command<i64, +i64>) -> i64 { 0 }").unwrap()).unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains("expected `)`, found `<`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn parse_choose_is_an_ordinary_identifier() {
        let p = parse_str("choose");
        match &p.decls[0].kind {
            Decl::Fn { body, .. } => {
                assert!(matches!(&body.kind, Expr::Ident(name) if name == "choose"));
            }
            _ => panic!("expected a function declaration"),
        }
    }

    #[test]
    fn parse_enum_variant_expression() {
        let p = parse_str("Color::Red");
        match &p.decls[0].kind {
            Decl::Fn { body, .. } => {
                assert!(matches!(&body.kind, Expr::Ident(name) if name == "Color::Red"));
            }
            _ => panic!("expected enum variant expression"),
        }
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
    fn parse_interaction() {
        let p = parse_str("f @ k");
        assert_eq!(p.decls.len(), 1);
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
    fn parse_data_pattern_with_shorthand() {
        let p = parse_str("match p { Point { x, y: rest } => 1 }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        let Expr::Match { arms, .. } = &body.kind else {
            panic!("expected match");
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Data { name, fields }
                if name == "Point" && fields.len() == 2
        ));
    }

    #[test]
    fn parse_negative_int_pattern() {
        let p = parse_str("match n { -3 => 1 }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else {
            panic!("expected main declaration");
        };
        let Expr::Match { arms, .. } = &body.kind else {
            panic!("expected match");
        };
        assert_eq!(arms[0].pattern, Pattern::Int(-3));
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
