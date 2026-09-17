//! The layout language and its printer.
//!
//! A document says where a line *may* break; the printer decides where it
//! *does*, by asking of each group whether it fits the width flat. This is
//! Wadler's algebra with Prettier's two additions: a hard line breaks every
//! group around it, and a conditional group offers several layouts, the
//! first that fits winning — which is how a trailing block is hugged,
//! `<v | f | select T {` … `}>`, rather than every stage taking a line.

#[derive(Debug, Clone)]
pub enum Doc {
    Text(String),
    /// A space flat, a newline broken.
    Line,
    /// Nothing flat, a newline broken.
    SoftLine,
    /// A newline, always.
    HardLine,
    /// A newline unless the line is still empty: what precedes a comment
    /// found where the grammar gave it no place of its own.
    FreshLine,
    Concat(Vec<Doc>),
    /// One more level of indentation for the lines inside.
    Nest(Box<Doc>),
    Group {
        doc: Box<Doc>,
        broken: bool,
    },
    /// Layouts to try in order, flat; the last is printed broken when none
    /// fits.
    Conditional(Vec<Doc>),
    IfBreak {
        broken: Box<Doc>,
        flat: Box<Doc>,
    },
}

pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}

pub fn concat(docs: Vec<Doc>) -> Doc {
    Doc::Concat(docs)
}

pub fn nest(doc: Doc) -> Doc {
    Doc::Nest(Box::new(doc))
}

/// A group, already broken when a hard line sits inside it.
pub fn group(doc: Doc) -> Doc {
    let broken = doc.has_hard();
    Doc::Group { doc: Box::new(doc), broken }
}

pub fn if_break(broken: Doc, flat: Doc) -> Doc {
    Doc::IfBreak { broken: Box::new(broken), flat: Box::new(flat) }
}

impl Doc {
    /// Whether a hard line sits inside, whatever layout is chosen. Every
    /// layout of a conditional group holds the same content, so its last
    /// answers for all.
    pub fn has_hard(&self) -> bool {
        match self {
            Doc::HardLine | Doc::FreshLine => true,
            Doc::Text(s) => s.contains('\n'),
            Doc::Line | Doc::SoftLine | Doc::IfBreak { .. } => false,
            Doc::Conditional(states) => states.last().is_some_and(Doc::has_hard),
            Doc::Concat(docs) => docs.iter().any(Doc::has_hard),
            Doc::Nest(doc) => doc.has_hard(),
            Doc::Group { broken, .. } => *broken,
        }
    }

    /// This document with the group it ends in broken, when it ends in one:
    /// the hugged layout of a trailing block.
    pub fn force_break(&self) -> Option<Doc> {
        match self {
            Doc::Group { doc, .. } => Some(Doc::Group { doc: doc.clone(), broken: true }),
            // A list that can hug its own last element does so: its hugged
            // layout is the one before its last.
            Doc::Conditional(states) => match states.len().checked_sub(2) {
                Some(hugged) => Some(states[hugged].clone()),
                None => states.last()?.force_break(),
            },
            Doc::Nest(doc) => Some(nest(doc.force_break()?)),
            // Past the closing text of a grouping, `(match … { … })`.
            Doc::Concat(docs) => {
                let at = docs.iter().rposition(|doc| !matches!(doc, Doc::Text(_)))?;
                let mut docs = docs.clone();
                docs[at] = docs[at].force_break()?;
                Some(Doc::Concat(docs))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Flat,
    Break,
}

type Cmd<'a> = (usize, Mode, &'a Doc);

fn width_of(s: &str) -> usize {
    s.chars().count()
}

/// Whether `next`, and what follows it up to the first newline, fits in
/// `remaining` columns.
fn fits(next: Cmd<'_>, rest: &[Cmd<'_>], mut remaining: isize) -> bool {
    let mut rest_at = rest.len();
    let mut stack = vec![next];
    loop {
        let Some((indent, mode, doc)) = stack.pop() else {
            if rest_at == 0 {
                return true;
            }
            rest_at -= 1;
            stack.push(rest[rest_at]);
            continue;
        };
        match doc {
            Doc::Text(s) => {
                if let Some(line) = s.split('\n').next()
                    && s.contains('\n')
                {
                    return remaining >= width_of(line) as isize;
                }
                remaining -= width_of(s) as isize;
                if remaining < 0 {
                    return false;
                }
            }
            Doc::HardLine | Doc::FreshLine => return true,
            Doc::Line | Doc::SoftLine if mode == Mode::Break => return true,
            Doc::Line => {
                remaining -= 1;
                if remaining < 0 {
                    return false;
                }
            }
            Doc::SoftLine => {}
            Doc::Concat(docs) => stack.extend(docs.iter().rev().map(|d| (indent, mode, d))),
            Doc::Nest(doc) => stack.push((indent + 1, mode, doc)),
            Doc::Group { doc, broken } => {
                stack.push((indent, if *broken { Mode::Break } else { mode }, doc))
            }
            Doc::Conditional(states) => {
                let state = if mode == Mode::Break { states.last() } else { states.first() };
                if let Some(state) = state {
                    stack.push((indent, mode, state));
                }
            }
            Doc::IfBreak { broken, flat } => {
                stack.push((indent, mode, if mode == Mode::Break { broken } else { flat }))
            }
        }
    }
}

pub fn print(doc: &Doc, width: usize, indent_width: usize) -> String {
    let mut out = String::new();
    // The column, and whether anything but indentation is on the line.
    let mut column = 0usize;
    let mut line_is_empty = true;
    let mut cmds: Vec<Cmd<'_>> = vec![(0, Mode::Break, doc)];
    let newline = |out: &mut String, indent: usize| {
        let kept = out.trim_end_matches(' ').len();
        out.truncate(kept);
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', indent * indent_width));
        indent * indent_width
    };
    while let Some((indent, mode, doc)) = cmds.pop() {
        match doc {
            Doc::Text(s) => {
                out.push_str(s);
                column = match s.rsplit_once('\n') {
                    Some((_, last)) => width_of(last),
                    None => column + width_of(s),
                };
                line_is_empty = false;
            }
            Doc::FreshLine if line_is_empty => {}
            Doc::HardLine | Doc::FreshLine => {
                column = newline(&mut out, indent);
                line_is_empty = true;
            }
            Doc::Line | Doc::SoftLine if mode == Mode::Break => {
                column = newline(&mut out, indent);
                line_is_empty = true;
            }
            Doc::Line => {
                out.push(' ');
                column += 1;
            }
            Doc::SoftLine => {}
            Doc::Concat(docs) => cmds.extend(docs.iter().rev().map(|d| (indent, mode, d))),
            Doc::Nest(doc) => cmds.push((indent + 1, mode, doc)),
            Doc::Group { doc: inner, broken } => {
                let flat = !*broken
                    && (mode == Mode::Flat
                        || fits(
                            (indent, Mode::Flat, inner),
                            &cmds,
                            width as isize - column as isize,
                        ));
                cmds.push((indent, if flat { Mode::Flat } else { Mode::Break }, inner));
            }
            // Inside a flat layout the choice is already made.
            Doc::Conditional(states) if mode == Mode::Flat => {
                if let Some(first) = states.first() {
                    cmds.push((indent, Mode::Flat, first));
                }
            }
            Doc::Conditional(states) => {
                let remaining = width as isize - column as isize;
                let chosen = states[..states.len().saturating_sub(1)]
                    .iter()
                    .find(|state| fits((indent, Mode::Flat, state), &cmds, remaining));
                match (chosen, states.last()) {
                    (Some(state), _) => cmds.push((indent, Mode::Flat, state)),
                    (None, Some(last)) => cmds.push((indent, Mode::Break, last)),
                    (None, None) => {}
                }
            }
            Doc::IfBreak { broken, flat } => {
                cmds.push((indent, mode, if mode == Mode::Break { broken } else { flat }))
            }
        }
    }
    let kept = out.trim_end().len();
    out.truncate(kept);
    out.push('\n');
    out
}
