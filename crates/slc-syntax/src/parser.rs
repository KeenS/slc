//! Recursive descent parser for Slant.

use crate::ast::*;
use crate::token::{Span, Token, TokenKind};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// The wrong-arrow guidance, one line per direction. The arrow says which
/// side of the mirror the scrutinee is on: data flows forward into an arm,
/// `=>`; a demand reaches back into it, `<=`.
const SELECT_ARROW: &str = "a `select` arm matches data, which flows forward: `pattern => command`";
const SELECT_LE: &str = "a `select` arm matches data, which flows forward: `pattern => command` — `<=` belongs to \
     `mu`, whose arms answer demands";
const MU_LE: &str = "a `mu` arm answers a demand, which reaches back: `copattern <= command`";
const MU_ARROW: &str = "a `mu` arm answers a demand, which reaches back: `copattern <= command` — `=>` belongs to \
     arms that match data";

/// Whether a pattern matches a continuation — a request shape — so that its
/// `match` arm writes `<=`.
fn pattern_is_copattern(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Dtor { .. } => true,
        Pattern::Or(items) => items.first().is_some_and(pattern_is_copattern),
        Pattern::Binding { pattern, .. } => pattern_is_copattern(pattern),
        _ => false,
    }
}

/// Read just enough of every `menu` declaration to disambiguate the one-arm
/// item shorthand before name resolution. Menus may be declared after their
/// uses (including in the appended prelude), so this is deliberately a
/// whole-token-stream pass rather than parser state accumulated in order.
fn collect_menu_items(tokens: &[Token]) -> HashMap<String, HashSet<String>> {
    let mut menus = HashMap::<String, HashSet<String>>::new();
    let mut pos = 0;
    while pos < tokens.len() {
        if tokens[pos].kind != TokenKind::Menu {
            pos += 1;
            continue;
        }
        let Some(TokenKind::Ident(name)) = tokens.get(pos + 1).map(|token| &token.kind) else {
            pos += 1;
            continue;
        };
        let mut cursor = pos + 2;
        while cursor < tokens.len() && tokens[cursor].kind != TokenKind::LBrace {
            cursor += 1;
        }
        if cursor == tokens.len() {
            break;
        }
        cursor += 1;
        let mut items = HashSet::new();
        let mut at_item_start = true;
        let mut depth = 0usize;
        while cursor < tokens.len() {
            match &tokens[cursor].kind {
                TokenKind::RBrace if depth == 0 => break,
                TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket => depth = depth.saturating_sub(1),
                TokenKind::Comma if depth == 0 => at_item_start = true,
                TokenKind::Ident(item)
                    if at_item_start
                        && tokens.get(cursor + 1).map(|token| &token.kind)
                            == Some(&TokenKind::Colon) =>
                {
                    items.insert(item.clone());
                    at_item_start = false;
                }
                _ => {}
            }
            cursor += 1;
        }
        menus.entry(name.clone()).or_default().extend(items);
        pos = cursor.saturating_add(1);
    }
    menus
}

/// Type parameters with their trait bounds.
type TypeParams = (Vec<String>, Vec<(String, String)>);

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
    /// The `pub` just read, awaiting the declaration it marks.
    pending_pub: bool,
    /// Menu names and their item labels, collected before parsing so the
    /// one-arm shorthand `mu M { item <= c }` stays distinct from the local
    /// continuation binder `mu A { k <= c }`.
    menu_items: HashMap<String, HashSet<String>>,
    in_block: bool,
    /// True where a following `{` opens a block, so an identifier before it
    /// is not a record literal.
    no_struct_literal: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        let menu_items = collect_menu_items(&tokens);
        Self {
            tokens,
            pos: 0,
            pending_pub: false,
            errors: Vec::new(),
            menu_items,
            in_block: false,
            no_struct_literal: false,
        }
    }

    fn type_is_menu_item(&self, ty: Option<&Node<TypeExpr>>, item: &str) -> bool {
        let name = match ty.map(|ty| &ty.kind) {
            Some(TypeExpr::Base(name) | TypeExpr::Apply(name, _)) => name,
            _ => return false,
        };
        let name = name.rsplit("::").next().unwrap_or(name);
        self.menu_items.get(name).is_some_and(|items| items.contains(item))
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

    /// The `pub` a declaration was written with, consumed by whichever
    /// `parse_*` runs next. Each takes it as its first act, before any
    /// nested declaration can set it again.
    fn take_pub(&mut self) -> bool {
        std::mem::take(&mut self.pending_pub)
    }

    fn parse_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let start = self.span_start();
        // A declaration is private to its module unless marked `pub`. One
        // in no module — the program's own, and the prelude's — is visible
        // everywhere, so `pub` there says nothing and is allowed.
        self.pending_pub = self.eat(&TokenKind::Pub);
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
                            is_public: true,
                            type_params: vec![],
                            bounds: vec![],
                            polarity: FunctionPolarity::Positive,
                            params: vec![],
                            return_type: None,
                            effects: EffectRow::default(),
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
                        is_public: true,
                        type_params: vec![],
                        bounds: vec![],
                        polarity: FunctionPolarity::Positive,
                        params: vec![],
                        return_type: None,
                        effects: EffectRow::default(),
                        body: e,
                    },
                })
            }
        }
    }

    fn parse_data(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
        let t = self.expect(TokenKind::Data, "`data`")?;
        let name = self.expect_name("data name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        if !bounds.is_empty() {
            return Err(ParseError {
                message: "a type declaration's parameters carry no bounds".into(),
                span: t.span,
            });
        }
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
        Ok(Node { span: t.span, kind: Decl::Data { name, is_public, type_params, fields } })
    }

    fn parse_form(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
        let t = self.expect(TokenKind::Form, "`form`")?;
        let name = self.expect_name("form name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        if !bounds.is_empty() {
            return Err(ParseError {
                message: "a type declaration's parameters carry no bounds".into(),
                span: t.span,
            });
        }
        let effects = self.parse_effect_row()?;
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
        Ok(Node {
            span: t.span,
            kind: Decl::Form { name, is_public, type_params, effects, fields },
        })
    }

    fn parse_menu(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
        let t = self.expect(TokenKind::Menu, "`menu`")?;
        let name = self.expect_ident("menu name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        if !bounds.is_empty() {
            return Err(ParseError {
                message: "a type declaration's parameters carry no bounds".into(),
                span: t.span,
            });
        }
        let effects = self.parse_effect_row()?;
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
        Ok(Node { span: t.span, kind: Decl::Menu { name, is_public, type_params, effects, items } })
    }

    fn parse_enum(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
        let t = self.expect(TokenKind::Enum, "`enum`")?;
        let name = self.expect_ident("enum name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        if !bounds.is_empty() {
            return Err(ParseError {
                message: "a type declaration's parameters carry no bounds".into(),
                span: t.span,
            });
        }
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
        Ok(Node { span: t.span, kind: Decl::Enum { name, is_public, type_params, variants } })
    }

    /// An optional effect row: `/ { E1, E2, ..R }`, or nothing for pure.
    /// `..R` names a row variable — declared as a generic parameter, it
    /// stands for the rest of the row, instantiated at each call.
    fn parse_effect_row(&mut self) -> Result<EffectRow, ParseError> {
        if !self.eat(&TokenKind::Slash) {
            return Ok(EffectRow::default());
        }
        self.expect(TokenKind::LBrace, "`{` after `/` in an effect row")?;
        let mut row = EffectRow::default();
        if self.eat(&TokenKind::RBrace) {
            return Ok(row);
        }
        loop {
            if self.eat(&TokenKind::DotDot) {
                row.tails.push(self.expect_ident("a row variable after `..`")?);
            } else {
                row.effects.push(self.expect_ident("an effect name")?);
            }
            if !self.eat(&TokenKind::Comma) {
                self.expect(TokenKind::RBrace, "`}` after the effect row")?;
                break;
            }
        }
        Ok(row)
    }

    fn parse_fn(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
        let t = self.expect(TokenKind::Fn, "`fn`")?;
        let name = self.expect_ident("function name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        let (params, separator) = self.parse_params_with()?;
        let (polarity, return_type) = self.parse_fn_arrow()?;
        let effects = self.parse_effect_row()?;
        let params = Self::group_for_polarity(params, separator, polarity, t.span)?;
        let body = self.parse_block()?;
        Ok(Node {
            span: t.span,
            kind: Decl::Fn {
                name,
                is_public,
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
        let is_public = self.take_pub();
        let t = self.expect(TokenKind::Command, "`command`")?;
        let name = self.expect_ident("`command` name")?;
        let (type_params, bounds) = self.parse_type_params_bounded()?;
        let (value_params, continuation_params) = self.parse_command_params()?;
        let return_type =
            if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?.kind) } else { None };
        if let Some(ref ty) = return_type
            && !matches!(ty, TypeExpr::Bottom)
            && !matches!(ty, TypeExpr::Base(name) if name == "Bottom")
        {
            return Err(ParseError {
                message: "a `command` returns `⊥` (`Bottom`); remove the arrow or use either unit spelling"
                    .into(),
                span: t.span,
            });
        }
        let effects = self.parse_effect_row()?;
        let body = self.parse_block()?;
        Ok(Node {
            span: t.span,
            kind: Decl::Command {
                name,
                is_public,
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
    fn parse_command_params(&mut self) -> Result<(Vec<Param>, Vec<Param>), ParseError> {
        let value_params = match self.peek_kind() {
            Some(TokenKind::LParen) => {
                let (params, separator) = self.parse_group("value")?;
                if separator.as_ref() == Some(&TokenKind::Amp) {
                    return Err(ParseError {
                        message: "a value group is a product, separated by `,`; `&` makes \
                                  the menu of exits, which is the second group"
                            .into(),
                        span: Span { start: 0, end: self.span_end() },
                    });
                }
                params
            }
            _ => Vec::new(),
        };
        if !self.eat(&TokenKind::Pipe) {
            return Ok((value_params, Vec::new()));
        }
        // A parameter in the second group is a continuation parameter because
        // of where it is declared, not because of anything that follows it.
        let (params, separator) = self.parse_group("continuation")?;
        if params.len() > 1 && separator.as_ref() != Some(&TokenKind::Amp) {
            return Err(ParseError {
                message: "a continuation group is a menu of exits, separated by `&`".into(),
                span: Span { start: 0, end: self.span_end() },
            });
        }
        Ok((value_params, Self::imply_continuation_signs(params)))
    }

    /// One group, which must hold something: an empty group is written by
    /// leaving it out.
    fn parse_group(&mut self, which: &str) -> Result<(Vec<Param>, Option<TokenKind>), ParseError> {
        let start = self.span_start();
        let (params, separator) = self.parse_params_with()?;
        if params.is_empty() {
            return Err(ParseError {
                message: format!("a group with no {which} parameters is not written"),
                span: Span { start, end: self.span_end() },
            });
        }
        Ok((params, separator))
    }

    fn parse_mod_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
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
        Ok(Node { span: t.span, kind: Decl::Mod { name, is_public, decls } })
    }

    fn parse_use_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let t = self.expect(TokenKind::Use, "`use`")?;
        let mut path = vec![self.expect_ident("a path to use")?];
        let mut imports = UseImports::Member;
        while self.eat(&TokenKind::ColonColon) {
            // `::*` — every variant of the enum the path names, bare.
            if self.eat(&TokenKind::Star) {
                imports = UseImports::Glob;
                break;
            }
            // `::{A, B}` — the listed variants, bare.
            if self.eat(&TokenKind::LBrace) {
                let mut names = Vec::new();
                loop {
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    names.push(self.expect_ident("a variant name")?);
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(TokenKind::RBrace, "`}` after the imported names")?;
                        break;
                    }
                }
                imports = UseImports::Names(names);
                break;
            }
            path.push(self.expect_ident("a path segment")?);
        }
        // `use list;` names a module already reachable at the root; it is
        // allowed, so a program can say what it draws on.
        let _ = self.eat(&TokenKind::Semicolon);
        Ok(Node { span: t.span, kind: Decl::Use { path, imports } })
    }

    fn parse_effect_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
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
        Ok(Node { span: t.span, kind: Decl::Effect { name, is_public, operations } })
    }

    fn parse_trait_decl(&mut self) -> Result<Node<Decl>, ParseError> {
        let is_public = self.take_pub();
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
        Ok(Node { span: t.span, kind: Decl::Trait { name, is_public, methods } })
    }

    /// A method signature: a `fn` or `command` header ending in `;`.
    fn parse_trait_method(&mut self) -> Result<TraitMethod, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Fn) => {
                self.pos += 1;
                let name = self.expect_ident("method name")?;
                let (params, separator) = self.parse_params_with()?;
                let (polarity, return_type) = self.parse_fn_arrow()?;
                let value_params = match polarity {
                    FunctionPolarity::Positive => params,
                    FunctionPolarity::Negative => Self::group_for_polarity(
                        params,
                        separator,
                        polarity,
                        Span { start: 0, end: 0 },
                    )?,
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
                let (value_params, continuation_params) = self.parse_command_params()?;
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
        let is_public = self.take_pub();
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
            kind: Decl::Const { name, is_public, ty: ty.kind, value },
        })
    }

    /// A declaration's parameters are its interface, so their types are
    /// written. A local `mu` is not an interface: its types may be left to
    /// the body that uses them.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        Ok(self.parse_params_with()?.0)
    }

    /// A parameter group, and the separator it used. `,` makes a product of
    /// values, `&` a menu of exits; which one is written says which group
    /// this is, and a group of one leaves it unsaid.
    fn parse_params_with(&mut self) -> Result<(Vec<Param>, Option<TokenKind>), ParseError> {
        let mut params = Vec::new();
        let mut separator = None;
        self.expect(TokenKind::LParen, "`(`")?;
        loop {
            if self.eat(&TokenKind::RParen) {
                break;
            }
            // A parameter binds a pattern: a group is a pattern with typed
            // leaves, and a bare name is the trivial one. A name is read as
            // a name rather than parsed as a pattern, so a parameter may
            // still be called `return`.
            let pattern =
                if self.tokens.get(self.pos + 1).map(|t| &t.kind) == Some(&TokenKind::Colon) {
                    Pattern::Ident(self.expect_ident("parameter name")?)
                } else {
                    self.parse_single_pattern()?
                };
            self.expect(TokenKind::Colon, "`:` — a declaration's parameters carry types")?;
            let ty = Some(self.parse_type()?.kind);
            params.push(Param { pattern, ty, is_continuation: false });
            let next = match self.peek_kind() {
                Some(kind @ (TokenKind::Comma | TokenKind::Amp)) => kind.clone(),
                _ => {
                    self.expect(TokenKind::RParen, "`)`")?;
                    break;
                }
            };
            if let Some(first) = &separator
                && first != &next
            {
                return Err(ParseError {
                    message: "a group separates its parameters one way: `,` for a product \
                              of values, `&` for a menu of exits"
                        .into(),
                    span: self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 }),
                });
            }
            separator = Some(next);
            self.pos += 1;
        }
        Ok((params, separator))
    }

    /// A `fn`'s one group, checked against the arrow that follows it: a
    /// positive function takes a product of values (`,`), a negative one a
    /// menu of exits (`&`), whose signs the group implies.
    fn group_for_polarity(
        params: Vec<Param>,
        separator: Option<TokenKind>,
        polarity: FunctionPolarity,
        span: Span,
    ) -> Result<Vec<Param>, ParseError> {
        match polarity {
            FunctionPolarity::Positive => {
                if separator.as_ref() == Some(&TokenKind::Amp) {
                    return Err(ParseError {
                        message: "`&` makes a menu of exits, so this group belongs to a \
                                  negative function — one written `<-`"
                            .into(),
                        span,
                    });
                }
                Ok(params)
            }
            FunctionPolarity::Negative => {
                if params.len() > 1 && separator.as_ref() != Some(&TokenKind::Amp) {
                    return Err(ParseError {
                        message: "a negative function's parameters are its exits, a menu \
                                  separated by `&`"
                            .into(),
                        span,
                    });
                }
                Ok(Self::imply_continuation_signs(params))
            }
        }
    }

    /// A continuation parameter's written type names what *reaches* it, so
    /// the group implies the sign: `(found: i64 & missing: String)` binds
    /// consumers of `i64` and `String`. A written sign stays legal.
    fn imply_continuation_signs(params: Vec<Param>) -> Vec<Param> {
        params
            .into_iter()
            .map(|mut p| {
                p.is_continuation = true;
                p.ty = p.ty.map(Self::imply_negative);
                p
            })
            .collect()
    }

    /// The sign a continuation position implies. It is supplied only where
    /// nothing was written and the shape itself does not carry one: a name,
    /// an applied declaration, a product, unit. A signed type, and any shape
    /// that is already a consumer — `⅋`, an arrow, `⊥`, a `dual` — says its
    /// own polarity and is left exactly as written. An `&` is the one shape
    /// the position reaches into: a menu of exits written out is still a
    /// menu of exits, so each item is one.
    fn imply_negative(ty: TypeExpr) -> TypeExpr {
        fn bare(node: Node<TypeExpr>) -> Box<Node<TypeExpr>> {
            Box::new(Node { span: node.span, kind: Parser::imply_negative(node.kind) })
        }
        match ty {
            TypeExpr::Base(_) | TypeExpr::Apply(..) | TypeExpr::Tensor(..) | TypeExpr::Unit => {
                TypeExpr::Negative(Box::new(Node { span: Span { start: 0, end: 0 }, kind: ty }))
            }
            TypeExpr::With(a, b) => TypeExpr::With(bare(*a), bare(*b)),
            signed => signed,
        }
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
            Some(TokenKind::LParen) => {
                self.pos += 1;
                // A paren holding only the separator is the nullary form:
                // `(&)` is the empty menu, ⊤, and `(,)` the empty tuple.
                if self.eat(&TokenKind::Amp) {
                    self.expect(TokenKind::RParen, "`)` after `(&`")?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: TypeExpr::Base("Top".into()),
                    });
                }
                if self.eat(&TokenKind::Comma) {
                    self.expect(TokenKind::RParen, "`)` after `(,`")?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: TypeExpr::Unit,
                    });
                }
                let left = self.parse_type()?;
                if self.eat(&TokenKind::Arrow) {
                    let right = self.parse_type()?;
                    let row = self.parse_effect_row()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    let fun = TypeExpr::Fun(
                        Box::new(left),
                        Box::new(Node {
                            span: Span { start, end: self.span_end() },
                            kind: right.kind,
                        }),
                    );
                    if row.is_empty() {
                        fun
                    } else {
                        TypeExpr::Effectful(
                            Box::new(Node {
                                span: Span { start, end: self.span_end() },
                                kind: fun,
                            }),
                            row,
                        )
                    }
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
                } else if self.eat(&TokenKind::Amp) {
                    let right = self.parse_type()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::With(
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
                } else if self.peek_kind() == Some(&TokenKind::Slash) {
                    // `(-A / {Exn})` — a latent row on the type itself: what
                    // consuming (or otherwise running) the value may perform.
                    let row = self.parse_effect_row()?;
                    self.expect(TokenKind::RParen, "`)`")?;
                    TypeExpr::Effectful(Box::new(left), row)
                } else {
                    self.expect(TokenKind::RParen, "`)`")?;
                    left.kind
                }
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
                // `List<i64>` — a declaration applied to type arguments.
                if self.peek_kind() == Some(&TokenKind::Lt) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type()?);
                        if self.eat(&TokenKind::Comma) {
                            continue;
                        }
                        self.expect(TokenKind::Gt, "`>` after type arguments")?;
                        break;
                    }
                    TypeExpr::Apply(s, args)
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
        Ok(Node { span: Span { start, end: self.span_end() }, kind })
    }

    pub fn parse_expr(&mut self) -> Result<Node<Expr>, ParseError> {
        // `|` binds more loosely than every operator, so `a + b | k` sends
        // the sum along. The chain is flat: composition is associative, and
        // the syntax says so rather than nesting.
        let start = self.span_start();
        let from_value = self.eat(&TokenKind::CutOpen);
        let first = self.parse_flow_stage()?;
        if !from_value && self.peek_kind() != Some(&TokenKind::Pipe) {
            return Ok(first);
        }
        let mut stages = vec![first];
        while self.eat(&TokenKind::Pipe) {
            stages.push(self.parse_flow_stage()?);
        }
        let into_consumer = self.eat(&TokenKind::CutClose);
        if from_value && into_consumer && stages.len() < 2 {
            return Err(ParseError {
                message: "a cut sends a value to a consumer, so it has both: \
                          `value | consumer⟩`"
                    .into(),
                span: Span { start, end: self.span_end() },
            });
        }
        let span = Span { start, end: self.span_end() };
        Ok(Node { span, kind: Expr::Flow { stages, from_value, into_consumer } })
    }

    /// One stage of a flow: everything that binds tighter than `|`.
    fn parse_flow_stage(&mut self) -> Result<Node<Expr>, ParseError> {
        let value = self.parse_binary(0)?;
        if self.peek_kind() == Some(&TokenKind::At) {
            return Err(ParseError {
                message: "`@` is gone: everything flows left to right through `|`, so a cut \
                          is `value | consumer⟩`"
                    .into(),
                span: Span { start: value.span.start, end: self.span_end() },
            });
        }
        Ok(value)
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
                // `mu` is uniformly `mu [Type] { arms }`. The type is what
                // the expression produces — a menu for the copattern form,
                // any type for the binder form — and may be left out when
                // the arms say it.
                let old_form_here = |parser: &Self, at: usize| {
                    matches!(parser.tokens.get(at).map(|t| &t.kind), Some(TokenKind::LParen))
                        && matches!(
                            parser.tokens.get(at + 1).map(|t| &t.kind),
                            Some(TokenKind::Ident(_))
                        )
                        && matches!(
                            parser.tokens.get(at + 2).map(|t| &t.kind),
                            Some(TokenKind::Colon) | Some(TokenKind::RParen)
                        )
                };
                // `mu(k)` / `mu name(k: -T)` — the retired parenthesised
                // binder. `(` can also open a tensor type, so only the
                // binder shape gets the guidance.
                if old_form_here(self, self.pos)
                    || (matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
                        && old_form_here(self, self.pos + 1))
                {
                    return Err(ParseError {
                        message: "the parenthesised `mu(k)` form is gone; bind the \
                                  continuation as an arm — `mu { k <= c }`, with the produced \
                                  type in front: `mu i64 { k <= c }`"
                            .into(),
                        span: self.peek().map(|t| t.span).unwrap_or(Span { start, end: start }),
                    });
                }
                let ty = match self.peek_kind() {
                    Some(TokenKind::LBrace) => None,
                    _ => Some(Box::new(self.parse_type()?)),
                };
                self.expect(TokenKind::LBrace, "`{` after `mu`")?;
                let mut arms = Vec::new();
                let mut shorthand_arms = Vec::new();
                loop {
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    let arm = self.pos;
                    if self.peek_kind() == Some(&TokenKind::Dot) {
                        let span = self.peek().expect("peeked a token").span;
                        self.skip_past_arm_list(arm);
                        return Err(ParseError {
                            message: "a `mu` copattern mirrors a menu field: write `item: out <= c` instead of `.item(out) <= c`".into(),
                            span,
                        });
                    }
                    let explicit_copattern = matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
                        && self.tokens.get(self.pos + 1).map(|t| &t.kind)
                            == Some(&TokenKind::Colon);
                    let pattern = match if explicit_copattern {
                        self.parse_mu_copattern()
                    } else {
                        self.parse_pattern()
                    } {
                        Ok(pattern) => pattern,
                        Err(e) => {
                            return Err(self.reversed_arm_error(arm, TokenKind::Le, MU_LE, e));
                        }
                    };
                    if !self.eat(&TokenKind::Le) {
                        let expected = self.expect(TokenKind::Le, "`<=` in `mu` arm").unwrap_err();
                        return Err(self.reversed_arm_error(
                            arm,
                            TokenKind::FatArrow,
                            MU_ARROW,
                            expected,
                        ));
                    }
                    let command = self.parse_expr()?;
                    arms.push(SelectArm { pattern, command });
                    shorthand_arms.push(!explicit_copattern);
                    if !self.eat(&TokenKind::Comma) {
                        self.expect(TokenKind::RBrace, "`}` after `mu` arm")?;
                        break;
                    }
                }
                let has_explicit_copattern =
                    arms.iter().zip(&shorthand_arms).any(|(arm, shorthand)| {
                        !shorthand && matches!(arm.pattern, Pattern::Dtor { .. })
                    });
                let multiple_arms = arms.len() > 1;
                for (arm, shorthand) in arms.iter_mut().zip(&shorthand_arms) {
                    let Pattern::Ident(label) = &arm.pattern else { continue };
                    if *shorthand
                        && (has_explicit_copattern
                            || multiple_arms
                            || self.type_is_menu_item(ty.as_deref(), label))
                    {
                        let label = label.clone();
                        arm.pattern = Pattern::Dtor {
                            dtor: label.clone(),
                            arg: Box::new(Pattern::Ident(label)),
                        };
                    }
                }
                // One binder arm — `mu { k <= c }` — is the atom form: it
                // captures the ambient continuation whole. Anything else is
                // the copattern form, a menu.
                let binder = match arms.as_slice() {
                    [SelectArm { pattern: Pattern::Ident(name), .. }] => Some(name.clone()),
                    [SelectArm { pattern: Pattern::Wildcard, .. }] => Some("__unused".to_string()),
                    _ => None,
                };
                if let Some(name) = binder {
                    let command = arms.pop().expect("matched one arm").command;
                    let param = Param::named(name, ty.map(TypeExpr::Negative), true);
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::Mu {
                            continuation_params: vec![param],
                            body: Box::new(command),
                        },
                    });
                }
                if arms
                    .iter()
                    .any(|arm| matches!(arm.pattern, Pattern::Ident(_) | Pattern::Wildcard))
                {
                    return Err(ParseError {
                        message: "a `mu` either binds its continuation with one arm, or \
                                  answers a menu's items — a binder arm stands alone"
                            .into(),
                        span: Span { start, end: self.span_end() },
                    });
                }
                Ok(Node {
                    span: Span { start, end: self.span_end() },
                    kind: Expr::CoMatch { ty, arms },
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
                        // The operation is a demand, and its carried
                        // continuation is bound the copattern way: after a
                        // colon — `op(args): k => body` — or not at all,
                        // for a clause that never resumes.
                        let resume = if self.eat(&TokenKind::Colon) {
                            self.expect_ident("the continuation binder after `:`")?
                        } else if let Some(TokenKind::Ident(name)) = self.peek_kind() {
                            let name = name.clone();
                            return Err(ParseError {
                                message: format!(
                                    "a clause binds its continuation after a colon:                                      `{op}(…): {name} => …` — or omits it when it                                      never resumes: `{op}(…) => …`"
                                ),
                                span: self
                                    .peek()
                                    .map(|t| t.span)
                                    .unwrap_or(Span { start: 0, end: 0 }),
                            });
                        } else {
                            // Never resumed: nothing in the body can name it.
                            "__never_resumed".to_string()
                        };
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
                    // A request arm matches a continuation, so its demand
                    // reaches back: `.item(out) <= e`. A data arm flows
                    // forward, `pattern => e`. (A request's payload is
                    // opaque, so a guard has nothing to test.)
                    let copattern = pattern_is_copattern(&pattern);
                    let guard = if !copattern && self.eat(&TokenKind::If) {
                        Some(self.parse_expr()?)
                    } else {
                        None
                    };
                    if copattern {
                        if self.peek_kind() == Some(&TokenKind::FatArrow) {
                            return Err(ParseError {
                                message: "a request arm matches a continuation — the demand \
                                          reaches back: `.item(out) <= e`"
                                    .into(),
                                span: self
                                    .peek()
                                    .map(|t| t.span)
                                    .unwrap_or(Span { start, end: start }),
                            });
                        }
                        self.expect(TokenKind::Le, "`<=` in a request arm")?;
                    } else {
                        if self.peek_kind() == Some(&TokenKind::Le) {
                            return Err(ParseError {
                                message: "a data arm flows forward: `pattern => e` — `<=` \
                                          belongs to arms that answer a continuation"
                                    .into(),
                                span: self
                                    .peek()
                                    .map(|t| t.span)
                                    .unwrap_or(Span { start, end: start }),
                            });
                        }
                        self.expect(TokenKind::FatArrow, "`=>`")?;
                    }
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
                    // A `select` arm matches data, and data flows forward
                    // into the command: `pattern => command`. The demands a
                    // `mu` answers reach back, `<=` — the arrow says which
                    // side of the mirror the scrutinee is on.
                    let arm = self.pos;
                    let pattern = match self.parse_pattern() {
                        Ok(pattern) => pattern,
                        Err(e) => {
                            return Err(self.reversed_arm_error(
                                arm,
                                TokenKind::FatArrow,
                                SELECT_ARROW,
                                e,
                            ));
                        }
                    };
                    if !self.eat(&TokenKind::FatArrow) {
                        let expected =
                            self.expect(TokenKind::FatArrow, "`=>` in `select` arm").unwrap_err();
                        return Err(self.reversed_arm_error(
                            arm,
                            TokenKind::Le,
                            SELECT_LE,
                            expected,
                        ));
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
                // A binder is a pattern; a bare name is the trivial one.
                // `parse_single_pattern`, not `parse_pattern`: an
                // or-pattern's `|` is the flow operator here.
                let pattern = self.parse_single_pattern()?;
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
                    kind: Expr::Let { pattern, ty, value: Box::new(value), body },
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
                // Inside parentheses a `{` can only open a record literal,
                // never the block a scrutinee position guards against.
                let outer_no_struct_literal = self.no_struct_literal;
                self.no_struct_literal = false;
                let parsed = self.parse_paren_expr(start);
                self.no_struct_literal = outer_no_struct_literal;
                parsed
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

    /// The body of a parenthesised expression, after the `(`: the nullary
    /// forms `(,)` and `(&)`, a grouping `(e)`, a tuple `(e1, e2, …)`, or a
    /// bundle of exits `(k1 & k2 & …)`.
    fn parse_paren_expr(&mut self, start: usize) -> Result<Node<Expr>, ParseError> {
        {
            {
                // A paren holding only the separator is the nullary form of
                // that connective: `(,)` the empty tuple, `(&)` the empty
                // menu — ⊤, whose value is unique.
                if self.eat(&TokenKind::Comma) {
                    self.expect(TokenKind::RParen, "`)` after `(,`")?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::Pair(Vec::new()),
                    });
                }
                if self.eat(&TokenKind::Amp) {
                    self.expect(TokenKind::RParen, "`)` after `(&`")?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::CoMatch {
                            ty: Some(Box::new(Node {
                                span: Span { start, end: self.span_end() },
                                kind: TypeExpr::Base("Top".into()),
                            })),
                            arms: Vec::new(),
                        },
                    });
                }
                if self.peek_kind() == Some(&TokenKind::RParen) {
                    return Err(ParseError {
                        message: "`()` is not a value; the empty tuple is `(,)` and the \
                                  empty menu `(&)`"
                            .into(),
                        span: Span { start, end: self.span_end() },
                    });
                }
                let first = self.parse_expr()?;
                // `(k1 & k2 & …)` — a bundle of exits.
                if self.peek_kind() == Some(&TokenKind::Amp) {
                    let mut items = vec![first];
                    while self.eat(&TokenKind::Amp) {
                        items.push(self.parse_expr()?);
                    }
                    self.expect(TokenKind::RParen, "`)` after a bundle")?;
                    return Ok(Node {
                        span: Span { start, end: self.span_end() },
                        kind: Expr::Bundle(items),
                    });
                }
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
        }
    }

    /// Explain the old arm order rather than reporting where its command
    /// failed to parse as a pattern. An arm holding a `=>` before it ends was
    /// written the other way round; the rest of the `select` is then skipped,
    /// so one arm in the old order reports one error.
    fn reversed_arm_error(
        &mut self,
        arm: usize,
        wrong: TokenKind,
        message: &str,
        fallback: ParseError,
    ) -> ParseError {
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
                _ if token.kind == wrong && depth == 0 => {
                    let span = token.span;
                    self.skip_past_arm_list(arm);
                    return ParseError { message: message.into(), span };
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

    /// A menu copattern mirrors the declaration's `label: Type` shape:
    /// `label: binder`, recursively for a nested menu item.
    fn parse_mu_copattern(&mut self) -> Result<Pattern, ParseError> {
        let dtor = self.expect_ident("menu item label")?;
        self.expect(TokenKind::Colon, "`:` after the menu item label")?;
        let arg = if matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
            && self.tokens.get(self.pos + 1).map(|token| &token.kind) == Some(&TokenKind::Colon)
        {
            self.parse_mu_copattern()?
        } else {
            self.parse_pattern()?
        };
        Ok(Pattern::Dtor { dtor, arg: Box::new(arg) })
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
            // `.item(p)` — a request shape: the demanded destructor, and a
            // pattern for the continuation the request carries.
            Some(TokenKind::Dot) => {
                self.pos += 1;
                let dtor = self.expect_ident("destructor name")?;
                self.expect(TokenKind::LParen, "`(` after the destructor")?;
                let arg = self.parse_pattern()?;
                self.expect(TokenKind::RParen, "`)`")?;
                Ok(Pattern::Dtor { dtor, arg: Box::new(arg) })
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
                // The nullary forms, as in expressions: `(,)` matches unit,
                // `(&)` the empty menu. Both bind nothing.
                if self.eat(&TokenKind::Comma) {
                    self.expect(TokenKind::RParen, "`)` after `(,`")?;
                    return Ok(Pattern::Tuple(Vec::new()));
                }
                if self.eat(&TokenKind::Amp) {
                    self.expect(TokenKind::RParen, "`)` after `(&`")?;
                    return Ok(Pattern::Bundle(Vec::new()));
                }
                if self.peek_kind() == Some(&TokenKind::RParen) {
                    return Err(ParseError {
                        message: "`()` is not a pattern; the empty tuple is `(,)` and the \
                                  empty menu `(&)`"
                            .into(),
                        span: self.peek().map(|t| t.span).unwrap_or(Span { start: 0, end: 0 }),
                    });
                }
                let mut items = vec![self.parse_pattern()?];
                // `(p & q)` — the bundle copattern, binding each exit.
                if self.peek_kind() == Some(&TokenKind::Amp) {
                    while self.eat(&TokenKind::Amp) {
                        items.push(self.parse_pattern()?);
                    }
                    self.expect(TokenKind::RParen, "`)` after a bundle pattern")?;
                    return Ok(Pattern::Bundle(items));
                }
                if self.eat(&TokenKind::Comma) {
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
                } else {
                    self.expect(TokenKind::RParen, "`)`")?;
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
            Decl::Fn { params, .. } if params.first().is_some_and(|p| p.name() == Some("return"))
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

        let p = parse_str("command step(x: +i32) | (k: -i32) -> Bottom { k(x) }");
        assert!(matches!(
            &p.decls[0].kind,
            Decl::Command { return_type: Some(TypeExpr::Base(name)), .. } if name == "Bottom"
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
        let p = parse_str("select Color { Red => 0 | return⟩, Green => 1 | return⟩ }");
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
    fn a_local_mu_may_leave_out_its_type() {
        let p = parse_str("mu { k <= 42 | k⟩ }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Mu { continuation_params, .. } = &body.kind else {
            panic!("expected a local mu: {:?}", body.kind)
        };
        assert_eq!(continuation_params[0].name(), Some("k"));
        assert_eq!(continuation_params[0].ty, None);

        // The produced type may be written in front; the binder then
        // consumes it — `mu i32 { k <= c }` gives `k` the type `-i32`.
        let p = parse_str("fn f() -> i32 { mu i32 { k <= 42 | k⟩ } }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected a block: {:?}", body.kind) };
        let Expr::Mu { continuation_params, .. } = &exprs[0].kind else {
            panic!("expected a local mu: {:?}", exprs[0].kind)
        };
        assert!(matches!(
            &continuation_params[0].ty,
            Some(TypeExpr::Negative(inner)) if matches!(&inner.kind, TypeExpr::Base(b) if b == "i32")
        ));
    }

    #[test]
    fn a_declaration_is_a_command_and_mu_is_the_expression() {
        // A declaration over parameters is a `command`; `mu` is the
        // expression capturing the ambient continuation.
        let p = parse_str("command f | (k: -i32) { 1 | k⟩ }");
        assert!(matches!(&p.decls[0].kind, Decl::Command { .. }));
        let p = parse_str("fn g() -> i32 { mu { k <= 1 | k⟩ } }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a fn") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected a block") };
        assert!(matches!(&exprs[0].kind, Expr::Mu { .. }));
    }

    #[test]
    fn the_parenthesised_mu_form_is_gone() {
        // `mu(k) { c }` and `mu name(k: -T) { c }` both point at the arm
        // syntax now.
        for source in
            ["fn f() -> i32 { mu(k: -i32) { 1 | k⟩ } }", "fn f() -> i32 { mu here(k) { 1 | k⟩ } }"]
        {
            let errors = parse(lex(source).unwrap()).unwrap_err();
            assert!(
                errors.iter().any(|e| e.message.contains("mu { k <= c }")),
                "{source}: {errors:?}"
            );
        }

        // A binder arm stands alone: it captures the whole continuation, so
        // a second arm has nothing left to answer.
        let errors =
            parse(lex("fn f() -> i32 { mu { _ <= 1, item: x <= 2 | x⟩ } }").unwrap()).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("binder arm stands alone")), "{errors:?}");
    }

    #[test]
    fn mu_copatterns_mirror_menu_fields() {
        let p = parse_str(
            "menu Stream { head: i32, tail: Stream }
             fn stream() -> Stream {
                 mu Stream {
                     head: out <= 1 | out⟩,
                     tail: head: out <= 2 | out⟩,
                 }
             }",
        );
        let Decl::Fn { body, .. } = &p.decls[1].kind else { panic!("expected a function") };
        let Expr::Block(items) = &body.kind else { panic!("expected a block") };
        let Expr::CoMatch { arms, .. } = &items[0].kind else { panic!("expected a menu mu") };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Dtor { dtor, arg }
                if dtor == "head" && matches!(&**arg, Pattern::Ident(name) if name == "out")
        ));
        assert!(matches!(
            &arms[1].pattern,
            Pattern::Dtor { dtor, arg }
                if dtor == "tail"
                    && matches!(&**arg, Pattern::Dtor { dtor, arg }
                        if dtor == "head"
                            && matches!(&**arg, Pattern::Ident(name) if name == "out"))
        ));
    }

    #[test]
    fn mu_item_label_is_its_default_binder() {
        let p = parse_str(
            "fn lazy() -> Lazy { mu Lazy { force <= 1 | force⟩ } }
             menu Lazy { force: i32 }",
        );
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a function") };
        let Expr::Block(items) = &body.kind else { panic!("expected a block") };
        let Expr::CoMatch { arms, .. } = &items[0].kind else { panic!("expected a menu mu") };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Dtor { dtor, arg }
                if dtor == "force" && matches!(&**arg, Pattern::Ident(name) if name == "force")
        ));

        let p = parse_str("fn captured() -> i32 { mu i32 { out <= 1 | out⟩ } }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a function") };
        let Expr::Block(items) = &body.kind else { panic!("expected a block") };
        assert!(matches!(&items[0].kind, Expr::Mu { .. }));
    }

    #[test]
    fn old_mu_destructor_syntax_points_to_the_field_form() {
        let errors = parse(lex("mu M { .item(out) <= 1 | out⟩ }").unwrap()).unwrap_err();
        assert!(errors[0].message.contains("item: out <= c"), "{errors:?}");
    }

    #[test]
    fn a_mu_writes_only_the_parameter_groups_it_has() {
        // No values: the group is left out, not written empty.
        let p = parse_str("command main | (exit: -i32) / {IO} { 0 | exit⟩ }");
        let Decl::Command { value_params, continuation_params, .. } = &p.decls[0].kind else {
            panic!("expected a mu declaration: {:?}", p.decls[0].kind)
        };
        assert!(value_params.is_empty());
        assert_eq!(continuation_params[0].name(), Some("exit"));
        assert!(continuation_params[0].is_continuation);

        // No continuations: the `|` goes with the group it introduces.
        let p = parse_str("command log(message: +String) { println(message) }");
        let Decl::Command { value_params, continuation_params, .. } = &p.decls[0].kind else {
            panic!("expected a mu declaration: {:?}", p.decls[0].kind)
        };
        assert_eq!(value_params[0].name(), Some("message"));
        assert!(continuation_params.is_empty());

        for source in
            ["command main() | (exit: -i32) { 0 | exit⟩ }", "command log(m: +String) | () { m }"]
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
        let p = parse_str("select { Red => 0 | return⟩ }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected a declaration") };
        let Expr::Select { ty, arms } = &body.kind else {
            panic!("expected a select: {:?}", body.kind)
        };
        assert!(ty.is_none());
        assert_eq!(arms.len(), 1);
    }

    #[test]
    fn parse_rejects_the_wrong_arrow_on_either_side() {
        // The arrow marks the scrutinee's side of the mirror: data flows
        // forward (`=>`), a demand reaches back (`<=`). Each wrong way gets
        // the guidance, not a token-soup error.
        let errors = parse(
            lex("enum Color { Red, Green }
                 fn k(return: -i32) <- Color {
                     select Color {
                         Red <= 0 | return⟩,
                         Green <= 1 | return⟩,
                     }
                 }")
            .unwrap(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("flows forward")), "errors: {errors:?}",);
        // One arm with the wrong arrow is one error, not one per arm after.
        assert_eq!(errors.len(), 1, "errors: {errors:?}");

        let errors = parse(
            lex("menu Config { retries: i64 }
                 fn f(k: -Config) -> -Config { match k { .retries(out) => .retries(out) } }")
            .unwrap(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("reaches back")), "errors: {errors:?}");

        let errors = parse(
            lex("menu Config { retries: i64 }
                 fn config() -> Config { mu Config { retries => 3 } }")
            .unwrap(),
        )
        .unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("reaches back")), "errors: {errors:?}");
    }

    #[test]
    fn parse_select_over_a_product() {
        // A product has one shape, so one arm, binding its components.
        let p = parse_str("select (+i64 ⊗ +String) { (end, text) => end | done⟩ }");
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
        let p = parse_str("select Reading { Reading { value: v, unit: u } => 0 | out⟩ }");
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
    fn parse_flow_binds_more_loosely_than_every_operator() {
        // `1 + 2 | k` sends the sum along, so the sum is one stage.
        let p = parse_str("fn main() -> i32 { 1 + 2 | k⟩ }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected fn") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected block") };
        let Expr::Flow { stages, .. } = &exprs[0].kind else {
            panic!("expected a flow: {:?}", exprs[0].kind)
        };
        assert_eq!(stages.len(), 2);
        assert!(matches!(stages[0].kind, Expr::BinOp { .. }), "value: {:?}", stages[0].kind);
        assert!(matches!(&stages[1].kind, Expr::Ident(name) if name == "k"));
    }

    #[test]
    fn parse_reads_a_chain_flat() {
        // Composition is associative, so the chain is one flat list of
        // stages — where a consumer may stand is the checker's rule, not
        // the grammar's.
        let p = parse_str("fn main() -> i32 { 1 | j | k⟩ }");
        let Decl::Fn { body, .. } = &p.decls[0].kind else { panic!("expected fn") };
        let Expr::Block(exprs) = &body.kind else { panic!("expected block") };
        let Expr::Flow { stages, .. } = &exprs[0].kind else {
            panic!("expected a flow: {:?}", exprs[0].kind)
        };
        assert_eq!(stages.len(), 3);
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
    fn a_type_application_parses_generically() {
        // `Name<…>` is ordinary generic syntax now — the removed `Command`
        // type former parses as an application and is rejected downstream,
        // where the checker finds no such declaration.
        let p = parse_str("fn f(x: Command<i64, +i64>) -> i64 { 0 }");
        let Decl::Fn { params, .. } = &p.decls[0].kind else { panic!("expected a fn") };
        assert!(matches!(
            &params[0].ty,
            Some(TypeExpr::Apply(name, args)) if name == "Command" && args.len() == 2
        ));
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
        let p = parse_str("fn f() -> i32 { mu { k <= 1 | k⟩ } }");
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
