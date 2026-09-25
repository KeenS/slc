//! The formatter proper: a walk over the token stream that mirrors the
//! parser's grammar and emits a layout document instead of a tree.
//!
//! It reads tokens rather than the AST because the AST is already
//! desugared — `k <= e` is a lambda there, `reset e` a handler, a grouping
//! parenthesis nothing at all — and has no comments. Walking the tokens
//! keeps every one as written, so the formatter only ever decides
//! whitespace, line breaks, and the optional separators: `;` between
//! statements and `,` between arms.
//!
//! The input has already parsed, so an unexpected token here is a bug in
//! this file's mirror of the grammar, reported rather than panicked on.

use crate::doc::{Doc, concat, group, if_break, nest, text};
use crate::trivia::{Comment, Stream, Tok};
use slc_syntax::token::{Span, TokenKind as T};

#[derive(Debug, Clone, PartialEq)]
pub struct FormatError {
    pub message: String,
    pub span: Span,
}

type R<X> = Result<X, FormatError>;

/// A layout, and what a chain needs to know of it.
struct Laid {
    doc: Doc,
    /// It ends in a block the line before may hug: `… | select T {` rather
    /// than the whole chain breaking.
    hug: bool,
    /// It is a binder stage, `x => e` or `k <= e`.
    binds: bool,
}

fn plain(doc: Doc) -> Laid {
    Laid { doc, hug: false, binds: false }
}

fn hugging(doc: Doc) -> Laid {
    Laid { doc, hug: true, binds: false }
}

/// One element of a list — an arm, a field, a statement, a declaration —
/// with the comments that belong to it.
struct Elem {
    /// Comments on their own lines above it.
    lead: Vec<Comment>,
    blank_before: bool,
    /// A blank line between the comments above and the element itself.
    blank_after_lead: bool,
    doc: Doc,
    /// A `// …` on the element's last line.
    trail: Option<Comment>,
    hug: bool,
}

#[derive(Clone, Copy)]
struct Style {
    sep: &'static str,
    /// `&` and `;` lead the line they break onto, as `|` does in a chain.
    leading_sep: bool,
    /// Whether the separator may follow the last element, broken.
    trailing: bool,
    /// The separator follows the last element even flat: `(e,)`.
    always_trailing: bool,
    /// Spaces inside the brackets when flat: `{ a, b }`.
    pad: bool,
    /// One element per line, whatever fits.
    force: bool,
    /// Whether a trailing block may be hugged.
    hug: bool,
    /// Whether the author's line break after the opening brace is kept: a
    /// list written down the page stays down the page.
    keep_break: bool,
}

const COMMAS: Style = Style {
    sep: ",",
    leading_sep: false,
    trailing: true,
    always_trailing: false,
    pad: false,
    force: false,
    hug: false,
    keep_break: false,
};
/// A list the grammar refuses a trailing comma in: type components, type
/// arguments, a row.
const BARE_COMMAS: Style = Style { trailing: false, ..COMMAS };
const ARGS: Style = Style { hug: true, ..COMMAS };
const FIELDS: Style = Style { pad: true, keep_break: true, ..COMMAS };
const ITEMS: Style = Style { sep: "", trailing: false, ..FIELDS };

fn leading(sep: &'static str) -> Style {
    Style { sep, leading_sep: true, trailing: false, hug: true, ..COMMAS }
}

/// The elements one per `Line`, with their comments and blank lines.
fn lines(elems: &[Elem], dangling: &[Comment], style: Style, force: bool) -> Vec<Doc> {
    let mut out = Vec::new();
    let count = elems.len();
    for (i, elem) in elems.iter().enumerate() {
        if i > 0 {
            out.push(if force { Doc::HardLine } else { Doc::Line });
            if elem.blank_before {
                out.push(Doc::HardLine);
            }
        }
        for (j, comment) in elem.lead.iter().enumerate() {
            if j > 0 && comment.newlines_before >= 2 {
                out.push(Doc::HardLine);
            }
            out.push(text(&comment.text));
            out.push(Doc::HardLine);
        }
        if elem.blank_after_lead {
            out.push(Doc::HardLine);
        }
        if style.leading_sep && i > 0 {
            out.push(text(format!("{} ", style.sep)));
        }
        out.push(elem.doc.clone());
        if !style.leading_sep && !style.sep.is_empty() {
            if i + 1 < count || style.always_trailing {
                out.push(text(style.sep));
            } else if style.trailing {
                out.push(if_break(text(style.sep), text("")));
            }
        }
        if let Some(comment) = &elem.trail {
            out.push(text(" "));
            out.push(text(&comment.text));
        }
    }
    for (j, comment) in dangling.iter().enumerate() {
        if j > 0 || count > 0 {
            out.push(Doc::HardLine);
            if comment.newlines_before >= 2 {
                out.push(Doc::HardLine);
            }
        }
        out.push(text(&comment.text));
    }
    out
}

fn render_list(
    open: Doc,
    elems: Vec<Elem>,
    dangling: Vec<Comment>,
    close: Doc,
    style: Style,
) -> Doc {
    if elems.is_empty() && dangling.is_empty() {
        return concat(vec![open, close]);
    }
    let has_comments = !dangling.is_empty()
        || elems.iter().any(|elem| !elem.lead.is_empty() || elem.trail.is_some());
    let has_blank = elems.iter().skip(1).any(|elem| elem.blank_before);
    let force = style.force || has_comments || has_blank;
    let edge = || match (force, style.pad) {
        (true, _) => Doc::HardLine,
        (false, true) => Doc::Line,
        (false, false) => Doc::SoftLine,
    };
    let mut inner = vec![edge()];
    inner.extend(lines(&elems, &dangling, style, force));
    let raw = concat(vec![open.clone(), nest(concat(inner)), edge(), close.clone()]);

    // The hugged layout: everything flat but the last element, which
    // breaks — `(ok & select String {` … `})`.
    let Some((last, rest)) = elems.split_last() else { return group(raw) };
    let may_hug = style.hug && !force && last.hug && !rest.iter().any(|elem| elem.doc.has_hard());
    let Some(forced) = last.doc.force_break().filter(|_| may_hug) else { return group(raw) };
    let last_is_hard = last.doc.has_hard();
    let mut hugged = vec![open];
    for elem in rest {
        hugged.push(elem.doc.clone());
        hugged.push(text(if style.leading_sep {
            format!(" {} ", style.sep)
        } else {
            format!("{} ", style.sep)
        }));
    }
    hugged.push(forced);
    if style.always_trailing {
        hugged.push(text(style.sep));
    }
    hugged.push(close);
    // A flat layout is offered only to what can be flat: a list whose last
    // element is already down the page hugs it or breaks.
    let mut states = if last_is_hard { Vec::new() } else { vec![raw.clone()] };
    states.extend([concat(hugged), group(raw)]);
    Doc::Conditional(states)
}

/// A declaration's header, which breaks at its parameter groups before it
/// breaks anywhere else: on one line; or with the last group down the
/// page; or with every group down the page. Left to themselves the groups
/// would each fit — what follows them can always break later — and the
/// break would land in the return type's `<…>`.
fn header(docs: Vec<Doc>, groups: &[usize]) -> Doc {
    let forced = |which: &[usize]| {
        let mut docs = docs.clone();
        for &at in which {
            docs[at] = docs[at].force_break()?;
        }
        Some(concat(docs))
    };
    let mut states = vec![concat(docs.clone())];
    states.extend(groups.last().and_then(|&last| forced(&[last])));
    if groups.len() > 1 {
        states.extend(forced(groups));
    }
    states.push(concat(docs.clone()));
    Doc::Conditional(states)
}

pub struct Formatter {
    toks: Vec<Tok>,
    /// Comments after the last token.
    trailing: Vec<Comment>,
    pos: usize,
    /// The parser's two context flags, mirrored: inside a block a `let` takes
    /// no `; rest`, and in a scrutinee a `{` opens the arms, not a record.
    in_block: bool,
    no_struct_literal: bool,
}

impl Formatter {
    pub fn new(stream: Stream) -> Self {
        Formatter {
            toks: stream.toks,
            trailing: stream.trailing,
            pos: 0,
            in_block: false,
            no_struct_literal: false,
        }
    }

    fn kind(&self) -> Option<&T> {
        self.kind_at(0)
    }

    fn kind_at(&self, ahead: usize) -> Option<&T> {
        self.toks.get(self.pos + ahead).map(|tok| &tok.kind)
    }

    fn at(&self, kind: &T) -> bool {
        self.kind() == Some(kind)
    }

    fn is_name(kind: Option<&T>) -> bool {
        matches!(kind, Some(T::Ident(_) | T::Reset))
    }

    fn error<X>(&self, expected: &str) -> R<X> {
        let (found, span) = match self.toks.get(self.pos) {
            Some(tok) => (format!("`{}`", tok.text), tok.span),
            None => ("end of input".to_string(), Span { start: 0, end: 0 }),
        };
        Err(FormatError {
            message: format!("the formatter expected {expected}, found {found}"),
            span,
        })
    }

    /// The comments before the next token that end their line, claimed by
    /// a list that gives them lines of their own. A `/* … */` sharing the
    /// token's line is left on it, to be written where it sits.
    fn take_comments(&mut self) -> Vec<Comment> {
        let Some(tok) = self.toks.get_mut(self.pos) else {
            return std::mem::take(&mut self.trailing);
        };
        if tok.comments.is_empty() {
            return Vec::new();
        }
        let mut ends_line: Vec<bool> =
            tok.comments.iter().skip(1).map(|next| next.newlines_before > 0).collect();
        ends_line.push(tok.newlines_before > 0);
        let owned = ends_line.iter().rposition(|ends| *ends).map_or(0, |last| last + 1);
        tok.comments.drain(..owned).collect()
    }

    /// A `// …` on the line of the token just consumed.
    fn take_trailing_comment(&mut self) -> Option<Comment> {
        let comments = match self.toks.get_mut(self.pos) {
            Some(tok) => &mut tok.comments,
            None => &mut self.trailing,
        };
        let trails =
            self.pos > 0 && comments.first().is_some_and(|c| c.newlines_before == 0 && c.is_line);
        trails.then(|| comments.remove(0))
    }

    /// Newlines before the next thing, comment or token.
    fn newlines_ahead(&self) -> usize {
        match self.toks.get(self.pos) {
            Some(tok) => tok.comments.first().map_or(tok.newlines_before, |c| c.newlines_before),
            None => self.trailing.first().map_or(0, |c| c.newlines_before),
        }
    }

    /// Comments nobody claimed, written where they were found: a comment in
    /// the middle of an expression keeps its place, and a `//` its newline.
    fn comment_docs(&mut self) -> Vec<Doc> {
        let Some(tok) = self.toks.get_mut(self.pos) else { return Vec::new() };
        let comments = std::mem::take(&mut tok.comments);
        let count = comments.len();
        let mut docs = Vec::new();
        for (i, comment) in comments.into_iter().enumerate() {
            if comment.newlines_before > 0 {
                docs.push(Doc::FreshLine);
            }
            docs.push(text(comment.text));
            let ends_line = comment.is_line || (i + 1 == count && tok.newlines_before > 0);
            docs.push(if ends_line { Doc::HardLine } else { text(" ") });
        }
        docs
    }

    /// Consume a token, as written.
    fn bump(&mut self) -> Doc {
        let mut docs = self.comment_docs();
        if let Some(tok) = self.toks.get(self.pos) {
            docs.push(text(&tok.text));
            self.pos += 1;
        }
        if docs.len() == 1 { docs.remove(0) } else { concat(docs) }
    }

    /// Consume a token the layout writes itself — an optional `;` or `,` —
    /// keeping only the comments before it.
    fn bump_silent(&mut self) -> Vec<Doc> {
        let docs = self.comment_docs();
        self.pos += 1;
        docs
    }

    fn expect(&mut self, kind: &T) -> R<Doc> {
        if self.at(kind) { Ok(self.bump()) } else { self.error(&format!("{kind}")) }
    }

    fn name(&mut self, what: &str) -> R<Doc> {
        if Self::is_name(self.kind()) { Ok(self.bump()) } else { self.error(what) }
    }

    /// `a::b::c`, no spaces.
    fn path(&mut self, what: &str) -> R<Doc> {
        let mut docs = vec![self.name(what)?];
        while self.at(&T::ColonColon) && Self::is_name(self.kind_at(1)) {
            docs.push(self.bump());
            docs.push(self.bump());
        }
        Ok(concat(docs))
    }

    // ── Lists ────────────────────────────────────────────────────────────

    /// The elements up to `close` — or the end of input — each with its
    /// comments. A separator is consumed wherever one is written; whether
    /// one is printed is the style's to say. `first` is an element already
    /// read, for the parenthesised forms that only show which they are
    /// after their first component.
    fn elems(
        &mut self,
        first: Option<Laid>,
        close: Option<&T>,
        seps: &[T],
        parse: &mut dyn FnMut(&mut Self) -> R<Laid>,
    ) -> R<(Vec<Elem>, Vec<Comment>, Option<T>)> {
        let mut elems = Vec::new();
        let mut used = None;
        let mut pending = first.map(|laid| Elem {
            lead: Vec::new(),
            blank_before: false,
            blank_after_lead: false,
            doc: laid.doc,
            trail: None,
            hug: laid.hug,
        });
        loop {
            let mut elem = match pending.take() {
                Some(elem) => elem,
                None => {
                    if self.kind().is_none() || self.kind() == close {
                        break;
                    }
                    let blank_before = self.newlines_ahead() >= 2;
                    let lead = self.take_comments();
                    let blank_after_lead =
                        !lead.is_empty() && self.toks[self.pos].newlines_before >= 2;
                    let Laid { doc, hug, .. } = parse(self)?;
                    Elem { lead, blank_before, blank_after_lead, doc, trail: None, hug }
                }
            };
            if let Some(kind) = self.kind().filter(|kind| seps.contains(*kind)).cloned() {
                used = Some(kind);
                let mut docs = self.bump_silent();
                if !docs.is_empty() {
                    docs.insert(0, elem.doc);
                    elem.doc = concat(docs);
                }
            }
            elem.trail = self.take_trailing_comment();
            elems.push(elem);
        }
        Ok((elems, self.take_comments(), used))
    }

    /// A bracketed list. Between braces the author's first line break is
    /// kept: a list written down the page stays down the page, and one
    /// written on a line stays there while it fits.
    fn bracketed(
        &mut self,
        (open, close): (&T, &T),
        seps: &[T],
        style: Style,
        parse: &mut dyn FnMut(&mut Self) -> R<Laid>,
    ) -> R<Laid> {
        let open = self.expect(open)?;
        let written_broken = style.keep_break && !self.at(close) && self.newlines_ahead() > 0;
        let (elems, dangling, _) = self.elems(None, Some(close), seps, parse)?;
        let count = elems.len();
        let close = self.expect(close)?;
        let style = Style { force: style.force || written_broken, ..style };
        let doc = render_list(open, elems, dangling, close, style);
        Ok(Laid { hug: count > 0, ..plain(doc) })
    }

    fn braces(
        &mut self,
        seps: &[T],
        style: Style,
        parse: &mut dyn FnMut(&mut Self) -> R<Laid>,
    ) -> R<Doc> {
        Ok(self.bracketed((&T::LBrace, &T::RBrace), seps, style, parse)?.doc)
    }

    // ── Declarations ─────────────────────────────────────────────────────

    pub fn program(&mut self) -> R<Doc> {
        let (elems, dangling, _) = self.elems(None, None, &[], &mut |f| f.decl().map(plain))?;
        Ok(concat(lines(&elems, &dangling, ITEMS, true)))
    }

    fn decl(&mut self) -> R<Doc> {
        let mut docs = Vec::new();
        if self.at(&T::Pub) {
            docs.extend([self.bump(), text(" ")]);
        }
        // `fn(` or `fn {` is a lambda, an expression at the top level.
        let is_lambda = matches!(self.kind_at(1), Some(T::LParen | T::LBrace));
        docs.push(match self.kind() {
            Some(T::Data | T::Form | T::Menu) => self.fields_decl()?,
            Some(T::Enum) => self.enum_decl()?,
            Some(T::Fn) if !is_lambda => self.fn_decl()?,
            Some(T::Command) => self.command_decl()?,
            Some(T::Const) => self.const_decl()?,
            Some(T::Mod) => {
                let mut head = vec![self.bump(), text(" "), self.name("a module name")?];
                // `mod name;`, a module in a file of its own.
                if self.at(&T::Semicolon) {
                    head.push(self.bump());
                    concat(head)
                } else {
                    head.push(text(" "));
                    self.with_items(head, &mut |f| f.decl().map(plain))?
                }
            }
            Some(T::Use) => self.use_decl()?,
            Some(T::Trait) => {
                let mut head =
                    vec![self.bump(), text(" "), self.name("a trait name")?, self.type_params()?];
                if self.at(&T::Colon) {
                    head.extend([self.bump(), text(" "), self.ty()?]);
                    while self.at(&T::Plus) {
                        head.extend([text(" "), self.bump(), text(" "), self.ty()?]);
                    }
                }
                head.push(text(" "));
                self.with_items(head, &mut |f| f.trait_item())?
            }
            Some(T::Impl) => {
                let mut head = vec![self.bump(), self.type_params()?, text(" ")];
                // `Into<i64>` is the trait applied to its arguments.
                head.extend([self.ty()?, text(" "), self.expect(&T::For)?]);
                head.extend([text(" "), self.ty()?, text(" ")]);
                self.with_items(vec![header(head, &[1])], &mut |f| f.impl_item())?
            }
            Some(T::Effect) => {
                let mut head = vec![self.bump(), text(" "), self.name("an effect name")?];
                head.extend([self.type_params()?, text(" ")]);
                self.with_items(head, &mut |f| f.operation().map(plain))?
            }
            // An expression at the top level, for scripting.
            _ => {
                let mut docs = vec![self.expr()?.doc];
                if self.at(&T::Semicolon) {
                    docs.push(self.bump());
                }
                concat(docs)
            }
        });
        Ok(concat(docs))
    }

    /// A header and its `{ … }` of items that end themselves: declarations,
    /// signatures.
    fn with_items(
        &mut self,
        mut head: Vec<Doc>,
        parse: &mut dyn FnMut(&mut Self) -> R<Laid>,
    ) -> R<Doc> {
        head.push(self.braces(&[], ITEMS, parse)?);
        Ok(concat(head))
    }

    /// `data`, `form` and `menu`: a name, and `label: Type` fields.
    fn fields_decl(&mut self) -> R<Doc> {
        let mut docs = vec![self.bump(), text(" "), self.name("a type name")?];
        docs.extend([self.type_params()?, self.effect_row()?, text(" ")]);
        docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| {
            let label = f.name("a field name")?;
            Ok(plain(concat(vec![label, f.expect(&T::Colon)?, text(" "), f.ty()?])))
        })?);
        Ok(concat(docs))
    }

    fn enum_decl(&mut self) -> R<Doc> {
        let mut docs = vec![self.bump(), text(" "), self.name("an enum name")?];
        docs.extend([self.type_params()?, text(" ")]);
        docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| {
            let mut docs = vec![f.name("a variant name")?];
            if f.at(&T::LParen) {
                let parens = (&T::LParen, &T::RParen);
                docs.push(
                    f.bracketed(parens, &[T::Comma], COMMAS, &mut |f| f.ty().map(plain))?.doc,
                );
            }
            Ok(plain(concat(docs)))
        })?);
        Ok(concat(docs))
    }

    fn fn_decl(&mut self) -> R<Doc> {
        let mut docs = vec![self.bump(), text(" "), self.name("a function name")?];
        docs.extend([self.type_params()?, self.params()?, text(" ")]);
        docs.extend([self.arrow()?, text(" "), self.ty()?, self.effect_row()?, text(" ")]);
        Ok(concat(vec![header(docs, &[4]), self.block()?]))
    }

    /// `->` or `<-`: the polarity a function declares.
    fn arrow(&mut self) -> R<Doc> {
        match self.kind() {
            Some(T::Arrow | T::ReverseArrow) => Ok(self.bump()),
            _ => self.error("`->` or `<-`"),
        }
    }

    fn command_decl(&mut self) -> R<Doc> {
        let mut docs = vec![self.bump(), text(" "), self.name("a command name")?];
        docs.push(self.type_params()?);
        let groups = self.command_groups(&mut docs)?;
        if self.at(&T::Arrow) {
            docs.extend([text(" "), self.bump(), text(" "), self.ty()?]);
        }
        docs.extend([self.effect_row()?, text(" ")]);
        Ok(concat(vec![header(docs, &groups), self.block()?]))
    }

    /// `(values) | (continuations)`, either side left out when empty.
    /// Returns where in `docs` the groups went.
    fn command_groups(&mut self, docs: &mut Vec<Doc>) -> R<Vec<usize>> {
        let mut groups = Vec::new();
        if self.at(&T::LParen) {
            groups.push(docs.len());
            docs.push(self.params()?);
        }
        if self.at(&T::Pipe) {
            docs.extend([text(" "), self.bump(), text(" ")]);
            groups.push(docs.len());
            docs.push(self.params()?);
        }
        Ok(groups)
    }

    /// `type Item;` or a method.
    fn trait_item(&mut self) -> R<Laid> {
        if self.at_word("type") {
            let mut docs = vec![self.bump(), text(" "), self.name("an associated type")?];
            docs.push(self.expect(&T::Semicolon)?);
            return Ok(plain(concat(docs)));
        }
        self.trait_method().map(plain)
    }

    /// `type Item = i64;` or a method.
    fn impl_item(&mut self) -> R<Laid> {
        if self.at_word("type") {
            let mut docs = vec![self.bump(), text(" "), self.name("an associated type")?];
            docs.extend([
                text(" "),
                self.expect(&T::Assign)?,
                text(" "),
                self.ty()?,
                self.expect(&T::Semicolon)?,
            ]);
            return Ok(plain(concat(docs)));
        }
        self.decl().map(plain)
    }

    fn at_word(&self, word: &str) -> bool {
        match self.kind() {
            Some(T::Ident(found)) => found == word,
            _ => false,
        }
    }

    fn trait_method(&mut self) -> R<Doc> {
        let is_command = self.at(&T::Command);
        let mut docs = vec![self.bump(), text(" "), self.name("a method name")?];
        let groups = if is_command {
            self.command_groups(&mut docs)?
        } else {
            docs.extend([self.params()?, text(" "), self.arrow()?, text(" "), self.ty()?]);
            vec![3]
        };
        if self.at(&T::Semicolon) {
            docs.push(self.expect(&T::Semicolon)?);
            Ok(header(docs, &groups))
        } else {
            docs.push(text(" "));
            Ok(concat(vec![header(docs, &groups), self.block()?]))
        }
    }

    /// An effect's operation: `fn op(params) -> T;`.
    fn operation(&mut self) -> R<Doc> {
        let mut docs = vec![self.expect(&T::Fn)?, text(" "), self.name("an operation name")?];
        docs.push(self.params()?);
        if self.at(&T::Arrow) {
            docs.extend([text(" "), self.bump(), text(" "), self.ty()?]);
        }
        docs.push(self.expect(&T::Semicolon)?);
        Ok(concat(docs))
    }

    fn const_decl(&mut self) -> R<Doc> {
        let mut docs = vec![self.bump(), text(" "), self.name("a constant name")?];
        docs.extend([self.expect(&T::Colon)?, text(" "), self.ty()?, text(" ")]);
        docs.extend([self.expect(&T::Assign)?, text(" "), self.expr()?.doc]);
        self.end_with_semicolon(&mut docs);
        Ok(concat(docs))
    }

    /// `;` where the grammar lets it be left out and the style does not.
    fn end_with_semicolon(&mut self, docs: &mut Vec<Doc>) {
        if self.at(&T::Semicolon) {
            docs.extend(self.bump_silent());
        }
        docs.push(text(";"));
    }

    fn use_decl(&mut self) -> R<Doc> {
        let mut docs = vec![self.bump(), text(" "), self.name("a path")?];
        while self.at(&T::ColonColon) {
            docs.push(self.bump());
            match self.kind() {
                Some(T::Star) => docs.push(self.bump()),
                Some(T::LBrace) => {
                    docs.push(self.braces(&[T::Comma], COMMAS, &mut |f| {
                        f.name("an imported name").map(plain)
                    })?)
                }
                _ => docs.push(self.name("a path segment")?),
            }
        }
        self.end_with_semicolon(&mut docs);
        Ok(concat(docs))
    }

    /// `<+T: Show + Ord, E>`, or nothing. `<-T>` opens with the one token `<-`.
    fn type_params(&mut self) -> R<Doc> {
        let open = match self.kind() {
            Some(T::Lt) => T::Lt,
            Some(T::ReverseArrow) => T::ReverseArrow,
            _ => return Ok(text("")),
        };
        let list = self.bracketed((&open, &T::Gt), &[T::Comma], COMMAS, &mut |f| {
            let mut docs = Vec::new();
            if matches!(f.kind(), Some(T::Plus | T::Minus | T::Star)) {
                docs.push(f.bump());
            }
            docs.push(f.name("a type parameter")?);
            // `T: Show + Into<i64>`. A bound is the trait and its arguments.
            if f.at(&T::Colon) {
                docs.extend([f.bump(), text(" "), f.ty()?]);
                while f.at(&T::Plus) {
                    docs.extend([text(" "), f.bump(), text(" "), f.ty()?]);
                }
            }
            Ok(plain(concat(docs)))
        })?;
        Ok(list.doc)
    }

    /// A parameter group: `,` makes a product of values, `&` a menu of
    /// exits, and the separator written says which.
    fn params(&mut self) -> R<Doc> {
        let open = self.expect(&T::LParen)?;
        let (elems, dangling, used) =
            self.elems(None, Some(&T::RParen), &[T::Comma, T::Amp], &mut |f| {
                // A name is read as a name, so a parameter may be `return`.
                let binder = if f.kind_at(1) == Some(&T::Colon) {
                    f.name("a parameter name")?
                } else {
                    f.single_pattern()?
                };
                Ok(plain(concat(vec![binder, f.expect(&T::Colon)?, text(" "), f.ty()?])))
            })?;
        let close = self.expect(&T::RParen)?;
        let style =
            if used == Some(T::Amp) { Style { hug: false, ..leading("&") } } else { COMMAS };
        Ok(render_list(open, elems, dangling, close, style))
    }

    // ── Types ────────────────────────────────────────────────────────────

    /// ` / {E1, ..R}`, or nothing.
    fn effect_row(&mut self) -> R<Doc> {
        if !self.at(&T::Slash) {
            return Ok(text(""));
        }
        Ok(concat(vec![text(" "), self.bump(), text(" "), self.row_body()?]))
    }

    fn row_body(&mut self) -> R<Doc> {
        self.braces(&[T::Comma], BARE_COMMAS, &mut |f| {
            if f.at(&T::DotDot) {
                Ok(plain(concat(vec![f.bump(), f.name("a row variable")?])))
            } else {
                f.ty().map(plain)
            }
        })
    }

    fn ty(&mut self) -> R<Doc> {
        match self.kind() {
            Some(T::Plus | T::Minus) => Ok(concat(vec![self.bump(), self.ty()?])),
            Some(T::Dual) => {
                let mut docs = vec![self.bump(), self.expect(&T::LParen)?];
                docs.extend([self.ty()?, self.expect(&T::RParen)?]);
                Ok(concat(docs))
            }
            Some(T::LParen) => self.paren_type(),
            Some(T::Ident(_)) => {
                let mut docs = vec![self.path("a type")?];
                let open = match self.kind() {
                    Some(T::Lt) => T::Lt,
                    Some(T::ReverseArrow) => T::ReverseArrow,
                    _ => return Ok(concat(docs)),
                };
                let args = self.bracketed((&open, &T::Gt), &[T::Comma], BARE_COMMAS, &mut |f| {
                    // `Item = i64` pins an associated type on a bound.
                    if matches!(f.kind(), Some(T::Ident(_))) && f.kind_at(1) == Some(&T::Assign) {
                        return Ok(plain(concat(vec![
                            f.bump(),
                            text(" "),
                            f.bump(),
                            text(" "),
                            f.ty()?,
                        ])));
                    }
                    match f.kind() {
                        // A row argument: `..E`, or a row written out.
                        Some(T::DotDot) => {
                            Ok(plain(concat(vec![f.bump(), f.name("a row variable")?])))
                        }
                        Some(T::LBrace) => f.row_body().map(plain),
                        _ => f.ty().map(plain),
                    }
                })?;
                docs.push(args.doc);
                Ok(concat(docs))
            }
            _ => self.error("a type"),
        }
    }

    fn paren_type(&mut self) -> R<Doc> {
        let open = self.expect(&T::LParen)?;
        let is_connective =
            |kind: Option<&T>| matches!(kind, Some(T::Comma | T::Pipe | T::Amp | T::Semicolon));
        // A paren holding only its separator is the nullary form: `(,)`.
        if is_connective(self.kind()) && self.kind_at(1) == Some(&T::RParen) {
            return Ok(concat(vec![open, self.bump(), self.bump()]));
        }
        let left = self.ty()?;
        if self.at(&T::Arrow) {
            let mut docs = vec![open, left, text(" "), self.bump(), text(" "), self.ty()?];
            docs.extend([self.effect_row()?, self.expect(&T::RParen)?]);
            return Ok(concat(docs));
        }
        if is_connective(self.kind()) {
            let connective = self.kind().cloned().expect("a connective was peeked");
            let style = match connective {
                T::Comma => BARE_COMMAS,
                T::Pipe => leading("|"),
                T::Amp => leading("&"),
                _ => leading(";"),
            };
            let (elems, dangling, _) =
                self.elems(Some(plain(left)), Some(&T::RParen), &[connective], &mut |f| {
                    f.ty().map(plain)
                })?;
            let close = self.expect(&T::RParen)?;
            return Ok(render_list(open, elems, dangling, close, Style { hug: false, ..style }));
        }
        // `(-A / {Exn})`, a latent row on the type, or a plain grouping.
        Ok(concat(vec![open, left, self.effect_row()?, self.expect(&T::RParen)?]))
    }

    // ── Expressions ──────────────────────────────────────────────────────

    /// `<value | stage | consumer>`: a flow, or the one stage it may be.
    fn expr(&mut self) -> R<Laid> {
        let open = if self.at(&T::Lt) { Some(self.bump()) } else { None };
        let first = self.stage()?;
        if open.is_none() && !self.at(&T::Pipe) {
            return Ok(first);
        }
        let mut pipes = Vec::new();
        let mut stages = vec![first];
        while self.at(&T::Pipe) {
            pipes.push(self.bump());
            stages.push(self.chain_stage()?);
        }
        let close = if self.at(&T::Gt) { Some(self.bump()) } else { None };

        // A line per stage with `|` leading. A binder names what the next
        // stage is given — `| x => (x, 1) | add` is one step — so the
        // stage after a binder shares its line.
        let mut broken: Vec<Doc> = open.iter().cloned().collect();
        let mut tail = Vec::new();
        for (i, pipe) in pipes.iter().enumerate() {
            tail.push(if stages[i].binds { text(" ") } else { Doc::Line });
            tail.extend([pipe.clone(), text(" "), stages[i + 1].doc.clone()]);
        }
        broken.extend([stages[0].doc.clone(), nest(concat(tail))]);
        broken.extend(close.clone());
        let broken = concat(broken);

        // Or one line, with the stages in `down` — the first, the last —
        // broken where they end in a block: `<v | f | select T {` … `}>`.
        let joined = |down: &[usize]| {
            let mut docs: Vec<Doc> = open.iter().cloned().collect();
            for (i, stage) in stages.iter().enumerate() {
                if i > 0 {
                    docs.extend([text(" "), pipes[i - 1].clone(), text(" ")]);
                }
                let hugs = down.contains(&i);
                docs.push(if hugs { stage.doc.force_break()? } else { stage.doc.clone() });
            }
            docs.extend(close.clone());
            Some(concat(docs))
        };
        let last = stages.len() - 1;
        let hard: Vec<usize> = (0..stages.len()).filter(|&i| stages[i].doc.has_hard()).collect();
        let mut states = Vec::new();
        if hard.is_empty() {
            states.push(broken.clone());
            states.extend(joined(&[last]).filter(|_| stages[last].hug));
            states.extend(joined(&[0]).filter(|_| stages[0].hug));
        } else if hard.iter().all(|&i| (i == 0 || i == last) && stages[i].hug) {
            // Stages already down the page: only the ends may be, and the
            // chain still reads along one line.
            states.extend(joined(&hard));
        }
        states.push(broken);
        Ok(plain(Doc::Conditional(states)))
    }

    /// A stage after `|`: an expression, or a binder naming what the stage
    /// is given — `x => e`, `k <= e`.
    fn chain_stage(&mut self) -> R<Laid> {
        let binds = matches!(self.kind(), Some(T::Ident(_)))
            && matches!(self.kind_at(1), Some(T::FatArrow | T::Le));
        if !binds {
            return self.stage();
        }
        let mut docs = vec![self.bump(), text(" "), self.bump(), text(" ")];
        let body = self.chain_stage()?;
        docs.push(body.doc);
        Ok(Laid { doc: concat(docs), hug: body.hug, binds: true })
    }

    /// Everything that binds tighter than `|`.
    fn stage(&mut self) -> R<Laid> {
        // A `-` touching a number is part of it, and stays touching.
        if self.at(&T::Minus) {
            return Ok(plain(concat(vec![self.bump(), self.bump()])));
        }
        let mut laid = self.primary()?;
        loop {
            match self.kind() {
                Some(T::LParen) => {
                    let parens = (&T::LParen, &T::RParen);
                    let args = self.bracketed(parens, &[T::Comma], ARGS, &mut |f| f.expr())?;
                    // A call hugs what its last argument hugs.
                    let hug = matches!(args.doc, Doc::Conditional(_));
                    laid = Laid { hug, ..plain(concat(vec![laid.doc, args.doc])) };
                }
                Some(T::Dot) => {
                    let mut docs = vec![laid.doc, self.bump()];
                    if self.at(&T::ColonColon) {
                        docs.push(self.bump());
                    }
                    if matches!(self.kind(), Some(T::Int(_))) || Self::is_name(self.kind()) {
                        docs.push(self.bump());
                    }
                    laid = plain(concat(docs));
                }
                _ => return Ok(laid),
            }
        }
    }

    fn primary(&mut self) -> R<Laid> {
        match self.kind() {
            // `.item(k)`, a request, and `::0(v)`, an alternative by position.
            Some(T::Dot | T::ColonColon) => {
                let mut docs = vec![self.bump(), self.bump(), self.expect(&T::LParen)?];
                docs.extend([self.expr()?.doc, self.expect(&T::RParen)?]);
                Ok(plain(concat(docs)))
            }
            Some(T::Int(_) | T::Float(_) | T::Str(_) | T::Char(_)) => Ok(plain(self.bump())),
            Some(T::Ident(_)) => {
                let name = self.path("a name")?;
                if self.no_struct_literal || !self.at(&T::LBrace) {
                    return Ok(plain(name));
                }
                let fields = self.braces(&[T::Comma], FIELDS, &mut |f| {
                    let mut docs = vec![f.name("a field name")?, f.expect(&T::Colon)?];
                    docs.extend([text(" "), f.expr()?.doc]);
                    Ok(plain(concat(docs)))
                })?;
                Ok(hugging(concat(vec![name, text(" "), fields])))
            }
            Some(T::Mu | T::Select) => {
                let is_mu = self.at(&T::Mu);
                let mut docs = vec![self.bump()];
                if !self.at(&T::LBrace) {
                    docs.extend([text(" "), self.ty()?]);
                }
                docs.push(text(" "));
                docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| f.arm(is_mu))?);
                Ok(hugging(concat(docs)))
            }
            Some(T::Fn) => {
                let mut docs = vec![self.bump()];
                // `fn { body }` takes nothing.
                if self.at(&T::LParen) {
                    docs.extend([self.bump(), self.name("a parameter name")?]);
                    if self.at(&T::Colon) {
                        docs.extend([self.bump(), text(" "), self.ty()?]);
                    }
                    docs.push(self.expect(&T::RParen)?);
                    if self.at(&T::Arrow) {
                        docs.extend([text(" "), self.bump(), text(" "), self.ty()?]);
                    }
                }
                docs.extend([text(" "), self.block()?]);
                Ok(hugging(concat(docs)))
            }
            Some(T::Reset) => {
                let docs = vec![self.bump(), text(" "), self.expr()?.doc];
                Ok(plain(concat(docs)))
            }
            Some(T::With) => {
                let mut docs = vec![self.bump(), text(" "), self.scrutinee()?, text(" ")];
                docs.extend([self.expect(&T::Handle)?, text(" "), self.expr()?.doc]);
                Ok(plain(concat(docs)))
            }
            Some(T::Handle) => {
                let mut docs = vec![self.bump(), text(" "), self.scrutinee()?, text(" ")];
                docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| f.clause())?);
                Ok(hugging(concat(docs)))
            }
            Some(T::Handler) => {
                let mut docs = vec![self.bump(), text(" ")];
                if self.at(&T::LBracket) {
                    let brackets = (&T::LBracket, &T::RBracket);
                    let effects = self.bracketed(brackets, &[T::Comma], COMMAS, &mut |f| {
                        f.path("an effect name").map(plain)
                    })?;
                    docs.extend([effects.doc, text(" ")]);
                } else if !self.at(&T::LBrace) {
                    docs.extend([self.path("an effect name")?, text(" ")]);
                }
                docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| f.clause())?);
                Ok(hugging(concat(docs)))
            }
            Some(T::Match) => {
                let mut docs = vec![self.bump(), text(" "), self.scrutinee()?, text(" ")];
                docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| f.arm(false))?);
                Ok(hugging(concat(docs)))
            }
            Some(T::Let) => self.let_expr(),
            Some(T::LParen) => {
                let outer = std::mem::replace(&mut self.no_struct_literal, false);
                let laid = self.paren_expr();
                self.no_struct_literal = outer;
                laid
            }
            Some(T::LBrace) => self.block().map(hugging),
            _ => self.error("an expression"),
        }
    }

    /// An expression after which a `{` opens arms, not a record literal.
    fn scrutinee(&mut self) -> R<Doc> {
        let outer = std::mem::replace(&mut self.no_struct_literal, true);
        let laid = self.expr();
        self.no_struct_literal = outer;
        Ok(laid?.doc)
    }

    /// `pattern => body`, or `copattern <= command`. Which arrow is the
    /// author's, and the parser has already held them to it.
    fn arm(&mut self, in_mu: bool) -> R<Laid> {
        let is_label =
            |f: &Self| matches!(f.kind(), Some(T::Ident(_))) && f.kind_at(1) == Some(&T::Colon);
        let mut docs = Vec::new();
        // A `mu` copattern mirrors the menu's `label: Type`, nested as far
        // as the labels go: `tail: head: out`.
        while in_mu && is_label(self) {
            docs.extend([self.bump(), self.bump(), text(" ")]);
        }
        docs.push(self.pattern()?);
        match self.kind() {
            Some(T::FatArrow | T::Le) => docs.extend([text(" "), self.bump(), text(" ")]),
            _ => return self.error("`=>` or `<=`"),
        }
        docs.push(self.expr()?.doc);
        Ok(plain(concat(docs)))
    }

    /// A handler's clause: `op(params): k => body`, `return(x) => body`, or
    /// `_ => forward`.
    fn clause(&mut self) -> R<Laid> {
        let mut docs = vec![self.path("an operation name")?];
        if self.at(&T::LParen) {
            let parens = (&T::LParen, &T::RParen);
            docs.push(
                self.bracketed(parens, &[T::Comma], COMMAS, &mut |f| {
                    f.name("an operation parameter").map(plain)
                })?
                .doc,
            );
        }
        if self.at(&T::Colon) {
            docs.extend([self.bump(), text(" "), self.name("a continuation binder")?]);
        }
        docs.extend([text(" "), self.expect(&T::FatArrow)?, text(" "), self.expr()?.doc]);
        Ok(plain(concat(docs)))
    }

    fn let_expr(&mut self) -> R<Laid> {
        let mut docs = vec![self.bump()];
        // `let+` and `let-`: the sign touches the keyword, so `let -1 = …`
        // still binds a negative literal pattern.
        let signed =
            matches!(self.kind(), Some(T::Plus | T::Minus)) && self.toks[self.pos].touches_previous;
        if signed {
            docs.push(self.bump());
        }
        docs.extend([text(" "), self.single_pattern()?]);
        if self.at(&T::Colon) {
            docs.extend([self.bump(), text(" "), self.ty()?]);
        }
        docs.extend([text(" "), self.expect(&T::Assign)?, text(" "), self.expr()?.doc]);
        // Outside a block the `;` is the binder's: `let x = v; rest`.
        if !self.in_block && self.at(&T::Semicolon) {
            docs.extend([self.bump(), Doc::HardLine, self.expr()?.doc]);
        }
        Ok(plain(concat(docs)))
    }

    /// After a `(`: a nullary form, a grouping, a tuple, a bundle of exits
    /// `(k1 & k2)`, or a form value `(k1 ; k2)`.
    fn paren_expr(&mut self) -> R<Laid> {
        let open = self.expect(&T::LParen)?;
        if matches!(self.kind(), Some(T::Comma | T::Amp)) && self.kind_at(1) == Some(&T::RParen) {
            return Ok(plain(concat(vec![open, self.bump(), self.bump()])));
        }
        let first = self.expr()?;
        let (sep, style) = match self.kind() {
            Some(T::Semicolon) => (T::Semicolon, leading(";")),
            Some(T::Amp) => (T::Amp, leading("&")),
            Some(T::Comma) => (T::Comma, ARGS),
            // A grouping is kept as written: the formatter adds and drops
            // no parentheses.
            _ => {
                let doc = concat(vec![open, first.doc, self.expect(&T::RParen)?]);
                return Ok(Laid { hug: first.hug, ..plain(doc) });
            }
        };
        let (elems, dangling, _) =
            self.elems(Some(first), Some(&T::RParen), &[sep], &mut |f| f.expr())?;
        let close = self.expect(&T::RParen)?;
        // One component and a comma is still a tuple, so the comma stays.
        let count = elems.len();
        let style = Style { always_trailing: count == 1, ..style };
        // A tuple or bundle is worth hugging for the block inside it —
        // `(ok & select String {` … `})` — not for its own parentheses.
        let hug = count > 1 && elems.iter().any(|elem| elem.hug);
        let doc = render_list(open, elems, dangling, close, style);
        Ok(Laid { hug, ..plain(doc) })
    }

    /// `{ e1; e2; e }`. Every statement but the last ends in `;`, and a
    /// `let` always does. A lone expression may share the braces' line.
    fn block(&mut self) -> R<Doc> {
        let open = self.expect(&T::LBrace)?;
        let written_broken = !self.at(&T::RBrace) && self.newlines_ahead() > 0;
        let outer = (self.in_block, self.no_struct_literal);
        (self.in_block, self.no_struct_literal) = (true, false);
        let mut lets = Vec::new();
        let parsed = self.elems(None, Some(&T::RBrace), &[T::Semicolon], &mut |f| {
            lets.push(f.at(&T::Let));
            f.expr()
        });
        (self.in_block, self.no_struct_literal) = outer;
        let (mut elems, dangling, _) = parsed?;
        let close = self.expect(&T::RBrace)?;
        let count = elems.len();
        for (i, (elem, is_let)) in elems.iter_mut().zip(&lets).enumerate() {
            if i + 1 < count || *is_let {
                elem.doc = concat(vec![elem.doc.clone(), text(";")]);
            }
        }
        let lone = count == 1 && !lets[0];
        let style = Style { force: written_broken || !lone, ..ITEMS };
        Ok(render_list(open, elems, dangling, close, style))
    }

    // ── Patterns ─────────────────────────────────────────────────────────

    fn pattern(&mut self) -> R<Doc> {
        let mut first = vec![self.single_pattern()?];
        if self.at(&T::DotDotEq) {
            first.extend([self.bump(), self.single_pattern()?]);
        }
        if self.at(&T::At) {
            first.extend([text(" "), self.bump(), text(" "), self.pattern()?]);
            return Ok(concat(first));
        }
        if !self.at(&T::Pipe) {
            return Ok(concat(first));
        }
        let mut rest = Vec::new();
        while self.at(&T::Pipe) {
            rest.extend([Doc::Line, self.bump(), text(" "), self.single_pattern()?]);
            if self.at(&T::DotDotEq) {
                rest.extend([self.bump(), self.single_pattern()?]);
            }
        }
        first.push(nest(concat(rest)));
        Ok(group(concat(first)))
    }

    fn single_pattern(&mut self) -> R<Doc> {
        let parens = (&T::LParen, &T::RParen);
        match self.kind() {
            // `.item(p)`, a request shape, and `::0(p)`, an alternative.
            Some(T::Dot | T::ColonColon) => {
                let mut docs = vec![self.bump(), self.bump(), self.expect(&T::LParen)?];
                docs.extend([self.pattern()?, self.expect(&T::RParen)?]);
                Ok(concat(docs))
            }
            Some(T::Minus) => Ok(concat(vec![self.bump(), self.single_pattern()?])),
            Some(T::Int(_) | T::Float(_) | T::Str(_) | T::Char(_) | T::DotDot) => Ok(self.bump()),
            Some(T::Ident(_)) => {
                let mut docs = vec![self.path("a pattern")?];
                if self.at(&T::LBrace) {
                    // A record pattern, with field shorthand: `P { x, y: p }`.
                    docs.push(text(" "));
                    docs.push(self.braces(&[T::Comma], FIELDS, &mut |f| {
                        let mut docs = vec![f.name("a field name")?];
                        if f.at(&T::Colon) {
                            docs.extend([f.bump(), text(" "), f.pattern()?]);
                        }
                        Ok(plain(concat(docs)))
                    })?);
                } else if self.at(&T::LParen) {
                    docs.push(
                        self.bracketed(parens, &[T::Comma], COMMAS, &mut |f| {
                            f.pattern().map(plain)
                        })?
                        .doc,
                    );
                }
                Ok(concat(docs))
            }
            Some(T::LParen) => {
                if matches!(self.kind_at(1), Some(T::Comma | T::Amp))
                    && self.kind_at(2) == Some(&T::RParen)
                {
                    return Ok(concat(vec![self.bump(), self.bump(), self.bump()]));
                }
                let open = self.bump();
                let first = self.pattern()?;
                let (sep, style) = match self.kind() {
                    Some(T::Amp) => (T::Amp, Style { hug: false, ..leading("&") }),
                    Some(T::Comma) => (T::Comma, COMMAS),
                    _ => return Ok(concat(vec![open, first, self.expect(&T::RParen)?])),
                };
                let (elems, dangling, _) =
                    self.elems(Some(plain(first)), Some(&T::RParen), &[sep], &mut |f| {
                        f.pattern().map(plain)
                    })?;
                let close = self.expect(&T::RParen)?;
                let style = Style { always_trailing: elems.len() == 1, ..style };
                Ok(render_list(open, elems, dangling, close, style))
            }
            _ => self.error("a pattern"),
        }
    }
}
