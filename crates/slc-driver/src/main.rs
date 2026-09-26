use std::path::PathBuf;
use std::process::ExitCode;

enum RunOutcome {
    Exit(i32),
}

/// The library, as source units appended after the program: the prelude
/// first — ordinary declarations every program sees unasked — then each
/// stdlib module, which a program reaches only through `use`. Every unit
/// goes through the same pipeline as user code. The prelude is the root
/// unit; every other library file is implicitly wrapped in a module named
/// after the file. See `prelude.sl` and `stdlib/` for what belongs where.
const LIBRARY: &[(&str, &str)] = &[
    ("prelude", include_str!("prelude.sl")),
    ("string", include_str!("stdlib/string.sl")),
    ("num", include_str!("stdlib/num.sl")),
    ("list", include_str!("stdlib/list.sl")),
    ("map", include_str!("stdlib/map.sl")),
    ("set", include_str!("stdlib/set.sl")),
    ("array", include_str!("stdlib/array.sl")),
    ("hashmap", include_str!("stdlib/hashmap.sl")),
    ("hashset", include_str!("stdlib/hashset.sl")),
    ("option", include_str!("stdlib/option.sl")),
    ("either", include_str!("stdlib/either.sl")),
    ("lazy", include_str!("stdlib/lazy.sl")),
    ("control", include_str!("stdlib/control.sl")),
    ("stream", include_str!("stdlib/stream.sl")),
    ("seq", include_str!("stdlib/seq.sl")),
    ("trace", include_str!("stdlib/trace.sl")),
    ("fs", include_str!("stdlib/fs.sl")),
];

/// The library units a program needs: the prelude always, and each stdlib
/// module the program reaches — by a path `list::…` or a `use list…` —
/// together with the modules those reach in turn, to a fixpoint. A unit
/// nothing reaches is never parsed or checked.
///
/// Reaching is read off tokens, not a parse: a name followed by `::`, or a
/// name after `use`. A false positive only loads a unit the program did not
/// need, and a lexing error loads nothing extra — the real lex reports it.
fn library_for(program: &str) -> Vec<(&'static str, &'static str)> {
    use slc_syntax::token::TokenKind;
    fn reached(text: &str) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        let Ok(tokens) = slc_syntax::lexer::lex(text) else { return out };
        for pair in tokens.windows(2) {
            match (&pair[0].kind, &pair[1].kind) {
                (TokenKind::Ident(name), TokenKind::ColonColon)
                | (TokenKind::Use, TokenKind::Ident(name)) => {
                    out.insert(name.clone());
                }
                _ => {}
            }
        }
        out
    }
    let (_, prelude) = LIBRARY[0];
    let mut wanted = reached(program);
    wanted.extend(reached(prelude));
    let mut included = vec![false; LIBRARY.len()];
    included[0] = true;
    loop {
        let mut grew = false;
        for (index, (name, text)) in LIBRARY.iter().enumerate() {
            if !included[index] && wanted.contains(*name) {
                included[index] = true;
                wanted.extend(reached(text));
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    LIBRARY.iter().zip(included).filter(|(_, needed)| *needed).map(|(unit, _)| *unit).collect()
}

/// The combined source and where each unit starts in it, so a span — a char
/// offset into the whole — can be named by its unit, line, and column.
///
/// The units, in order: the program's own file; each module file it
/// reaches through `mod name;`, as it is reached; then the library units it
/// needs. Every unit goes through the same pipeline as the program's text.
struct SourceMap {
    text: String,
    /// `(name as a diagnostic gives it, char offset of the unit's first
    /// char)`, in order. The first is the program's own file.
    units: Vec<(String, usize)>,
}

impl SourceMap {
    /// Append a unit on a line of its own, and say where it starts. The
    /// program comes first, so its spans — and its diagnostics' line
    /// numbers — are untouched.
    fn push_unit(&mut self, name: String, unit: &str) -> usize {
        if !self.units.is_empty() {
            self.text.push('\n');
        }
        let from = self.text.chars().count();
        self.units.push((name, from));
        self.text.push_str(unit);
        from
    }

    /// The char offsets at which each library unit starts — the boundaries
    /// resolution scopes imports by.
    fn boundaries(&self) -> Vec<usize> {
        self.units.iter().skip(1).map(|(_, from)| *from).collect()
    }

    /// `unit:line:column \`snippet\`` for a span. The line and column are
    /// within the unit, not the combined text, and the unit is named only
    /// when it is not the program's own file.
    fn locate(&self, span: slc_syntax::token::Span) -> String {
        let (unit, from) =
            self.units.iter().rev().find(|(_, from)| span.start >= *from).cloned().unwrap();
        // Spans count chars — the surface has multi-byte glyphs — so the
        // text is walked by char to find the byte range to show.
        let byte_at = |chars: usize| {
            self.text.char_indices().nth(chars).map(|(b, _)| b).unwrap_or(self.text.len())
        };
        let (start, end) = (byte_at(span.start), byte_at(span.end.max(span.start)));
        let before = &self.text[byte_at(from)..start];
        let line = before.matches('\n').count() + 1;
        let column = before
            .rfind('\n')
            .map(|i| before[i..].chars().count())
            .unwrap_or(before.chars().count() + 1);
        let snippet = &self.text[start..end];
        if from == 0 {
            format!("{line}:{column} `{snippet}`")
        } else {
            format!("{unit}:{line}:{column} `{snippet}`")
        }
    }
}

impl SourceMap {
    /// ` (at line:column \`snippet\`)` for a lex or parse error. A syntax
    /// error's span can run to the end of the file — an unterminated string
    /// does — so only its first line is quoted. One with no extent, as at
    /// the end of input, has no place to name, and none is claimed.
    fn locate_syntax(&self, span: slc_syntax::token::Span) -> String {
        if span.end <= span.start {
            return String::new();
        }
        let located = self.locate(span);
        match located.split_once('\n') {
            Some((first_line, _)) => format!(" (at {first_line}…`)"),
            None => format!(" (at {located})"),
        }
    }
}

const MAIN_ENTRY_POINT_ERROR: &str = "entry point must be `proc main | (exit: -i32) / {IO} { ... }`: a proc with no value \
     parameters and one continuation, the exit status";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version") {
        println!("slc {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let usage = || {
        eprintln!("usage: slc run [--fuel N] <file.sl>");
        eprintln!("       slc check <file.sl>...");
        eprintln!("       slc fmt [--check | --stdout] <file.sl>...");
        ExitCode::FAILURE
    };
    if let Some(check_at) = args.iter().position(|a| a == "check") {
        let files = &args[check_at + 1..];
        if files.is_empty() || files.iter().any(|f| f.starts_with("--")) {
            return usage();
        }
        return check_files(files);
    }
    if let Some(fmt_at) = args.iter().position(|a| a == "fmt") {
        return match format_files(&args[fmt_at + 1..]) {
            Some(code) => code,
            None => usage(),
        };
    }
    let Some(run_at) = args.iter().position(|a| a == "run") else {
        return usage();
    };
    // A run is bounded only by memory unless `--fuel N` caps its machine
    // steps, for a test or a program that might diverge.
    let mut fuel = usize::MAX;
    let mut file = None;
    let mut rest = args[run_at + 1..].iter();
    while let Some(arg) = rest.next() {
        if arg == "--fuel" {
            match rest.next().and_then(|n| n.parse().ok()) {
                Some(n) => fuel = n,
                None => return usage(),
            }
        } else if file.is_none() {
            file = Some(PathBuf::from(arg));
        } else {
            return usage();
        }
    }
    let Some(file) = file else {
        return usage();
    };

    // A continuation-passing program nests as deeply as its control flow,
    // and the evaluator walks the tree on the host stack, so give it room.
    let outcome = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run_file(&file, fuel))
        .expect("failed to start the evaluator")
        .join()
        .unwrap_or_else(|_| Err("evaluation ran out of stack".into()));

    match outcome {
        Ok(RunOutcome::Exit(code)) => {
            let code: Result<u8, _> = code.try_into();
            code.map(ExitCode::from).unwrap_or(ExitCode::FAILURE)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `slc fmt`: rewrite each file in the one layout. `--check` writes nothing
/// and fails if any file would change, for CI; `--stdout` prints the result
/// instead, for an editor. `None` when the arguments make no sense.
fn format_files(args: &[String]) -> Option<ExitCode> {
    let (flags, files): (Vec<&String>, Vec<&String>) =
        args.iter().partition(|a| a.starts_with("--"));
    let check = flags.iter().any(|f| *f == "--check");
    let stdout = flags.iter().any(|f| *f == "--stdout");
    let known = flags.iter().all(|f| *f == "--check" || *f == "--stdout");
    if files.is_empty() || !known || (check && stdout) {
        return None;
    }
    let mut failed = false;
    for file in files {
        let result = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {file}: {e}"))
            .and_then(|source| {
                let formatted =
                    slc_fmt::format_source(&source).map_err(|e| format!("{file}: {e}"))?;
                Ok((source, formatted))
            })
            .and_then(|(source, formatted)| {
                if stdout {
                    print!("{formatted}");
                } else if source != formatted && check {
                    return Err(format!("{file} is not formatted: run `slc fmt {file}`"));
                } else if source != formatted {
                    std::fs::write(file, formatted)
                        .map_err(|e| format!("cannot write {file}: {e}"))?;
                }
                Ok(())
            });
        if let Err(message) = result {
            eprintln!("error: {message}");
            failed = true;
        }
    }
    Some(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// Keep only the first declaration for each top-level name, per namespace:
/// values (`fn`, `command`, `const`) in one, type declarations in the other.
/// The program's declarations precede the prelude's, so its definitions win.
/// The program's own declarations shadow the library's: the library is
/// appended, so of two declarations of one name the first — the program's
/// — is kept. That holds for a module too, whole.
fn shadow_prelude(mut program: slc_syntax::ast::Program) -> slc_syntax::ast::Program {
    use slc_syntax::ast::Decl;
    let mut values = std::collections::HashSet::new();
    let mut types = std::collections::HashSet::new();
    let mut modules = std::collections::HashSet::new();
    program.decls.retain(|d| match &d.kind {
        Decl::Fn { name, .. } | Decl::Command { name, .. } | Decl::Const { name, .. } => {
            values.insert(name.clone())
        }
        Decl::Data { name, .. }
        | Decl::Enum { name, .. }
        | Decl::Menu { name, .. }
        | Decl::Form { name, .. } => types.insert(name.clone()),
        // A program's `mod list` shadows the stdlib's whole, as its `fn
        // length` would shadow the prelude's: the first declaration wins.
        Decl::Mod { name, .. } => modules.insert(name.clone()),
        _ => true,
    });
    program
}

/// Lex and parse a source map's text, reporting what is wrong with where.
/// What a compiler phase found wrong: one entry per diagnostic, since a
/// diagnostic that quotes several lines of source is itself several lines.
type Diagnostics = Vec<String>;

use slc_syntax::token::{Span, Token, TokenKind};

/// Lex one unit on its own and shift its spans to where its text sits in
/// the combined source. No unit's lexing can see another's text, so a
/// string one file leaves open cannot close on the next file's first quote.
fn lex_unit(map: &SourceMap, unit: &str, from: usize) -> Result<Vec<Token>, Diagnostics> {
    let shift = |span: Span| Span { start: span.start + from, end: span.end + from };
    let tokens = slc_syntax::lexer::lex(unit)
        .map_err(|e| vec![format!("{}{}", e.message, map.locate_syntax(shift(e.span)))])?;
    Ok(tokens.into_iter().map(|t| Token { kind: t.kind, span: shift(t.span) }).collect())
}

fn parse_units(
    map: &SourceMap,
    tokens: Vec<Token>,
) -> Result<slc_syntax::ast::Program, Diagnostics> {
    slc_syntax::parser::parse(tokens).map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("parse error: {}{}", e.message, map.locate_syntax(e.span)))
            .collect::<Vec<_>>()
    })
}

/// A program's own files, loading: the map they are appended to, and the
/// files already in it.
struct Loader {
    map: SourceMap,
    loaded: std::collections::HashSet<PathBuf>,
}

impl Loader {
    /// Load a source file as the next unit and return its tokens, with each
    /// `mod name;` replaced by `mod name {`, that file's tokens, and `}` —
    /// so the parser sees one ordinary program, and reads every unit's
    /// `menu`s before it parses any of them.
    ///
    /// `children` is where this file's own `mod name;` are looked for:
    /// beside the program, and in `dir/m/` for a module file `dir/m.sl`. An
    /// inline `mod a { … }` adds `a/`. See
    /// `docs/design-notes/file-modules.md`.
    fn load(
        &mut self,
        path: &std::path::Path,
        children: &std::path::Path,
    ) -> Result<Vec<Token>, Diagnostics> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| vec![format!("cannot read {}: {e}", path.display())])?;
        let from = self.map.push_unit(path.display().to_string(), &source);
        let tokens = lex_unit(&self.map, &source, from)?;
        // The file's syntax is checked on its own, so a brace it leaves
        // open is reported here and not wherever the next unit closes it.
        parse_units(&self.map, tokens.clone())?;

        let mut out = Vec::with_capacity(tokens.len());
        // One entry per open `{`: the inline module it opens, if it opens one.
        let mut nesting: Vec<Option<&str>> = Vec::new();
        for (i, token) in tokens.iter().enumerate() {
            let declares = |at: usize| match (tokens.get(at).map(|t| &t.kind), tokens.get(at + 1)) {
                (Some(TokenKind::Mod), Some(Token { kind: TokenKind::Ident(name), .. })) => {
                    Some(name)
                }
                _ => None,
            };
            match &token.kind {
                TokenKind::LBrace => {
                    nesting.push(i.checked_sub(2).and_then(declares).map(String::as_str));
                }
                TokenKind::RBrace => {
                    nesting.pop();
                }
                TokenKind::Semicolon => {
                    if let Some(name) = i.checked_sub(2).and_then(declares) {
                        let mut dir = children.to_path_buf();
                        dir.extend(nesting.iter().flatten());
                        let file = dir.join(format!("{name}.sl"));
                        let declared =
                            Span { start: tokens[i - 2].span.start, end: token.span.end };
                        let at = self.map.locate_syntax(declared);
                        if !file.is_file() {
                            return Err(vec![format!(
                                "module `{name}` is declared in a file of its own, and there is \
                                 no {}{at}",
                                file.display()
                            )]);
                        }
                        if !self.loaded.insert(file.clone()) {
                            return Err(vec![format!(
                                "{} is already loaded: a file is one module, declared once{at}",
                                file.display()
                            )]);
                        }
                        let body = self.load(&file, &dir.join(name))?;
                        out.push(Token { kind: TokenKind::LBrace, span: token.span });
                        out.extend(body);
                        out.push(Token { kind: TokenKind::RBrace, span: token.span });
                        continue;
                    }
                }
                _ => {}
            }
            out.push(token.clone());
        }
        Ok(out)
    }
}

/// Every unit of a program, as one token stream and the map that names its
/// spans: the program's file and the module files it reaches, then the
/// library units those need.
fn load_program(path: &std::path::Path) -> Result<(SourceMap, Vec<Token>), Diagnostics> {
    let mut loader = Loader {
        map: SourceMap { text: String::new(), units: Vec::new() },
        loaded: std::collections::HashSet::from([path.to_path_buf()]),
    };
    let beside = path.parent().unwrap_or(std::path::Path::new(""));
    let mut tokens = loader.load(path, beside)?;
    let mut map = loader.map;
    for (name, unit) in library_for(&map.text) {
        let from = map.push_unit(format!("{name}.sl"), unit);
        let unit_tokens = lex_unit(&map, unit, from)?;
        if name == "prelude" {
            tokens.extend(unit_tokens);
        } else {
            // Library files follow the same default-module convention as
            // `mod name;` files: `stdlib/name.sl` supplies `mod name`.
            let span = Span { start: from, end: from };
            tokens.extend([
                Token { kind: TokenKind::Mod, span },
                Token { kind: TokenKind::Ident(name.to_owned()), span },
                Token { kind: TokenKind::LBrace, span },
            ]);
            tokens.extend(unit_tokens);
            tokens.push(Token { kind: TokenKind::RBrace, span });
        }
    }
    Ok((map, tokens))
}

/// `slc check`: every compiler phase over each file, and no run — what an
/// editor asks on every save, and what a program that would not stop can
/// still be asked. A file with no `main` is a library and checks; a `main`
/// of the wrong shape is refused as `run` would refuse it.
fn check_files(files: &[String]) -> ExitCode {
    let mut failed = false;
    for file in files {
        // Checking recurses as deeply as the program nests, like the run.
        let path = PathBuf::from(file);
        let result = std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                let compiled = compile_file(&path)?;
                if declares_main(&compiled.program) {
                    validate_main(&compiled.program).map_err(|message| vec![message])?;
                }
                Ok(())
            })
            .expect("failed to start the checker")
            .join()
            .unwrap_or_else(|_| Err::<(), Diagnostics>(vec!["checking ran out of stack".into()]));
        if let Err(diagnostics) = result {
            for diagnostic in diagnostics {
                eprintln!("error: {file}: {diagnostic}");
            }
            failed = true;
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// A program through every compiler phase: what `check` reports on and
/// `run` goes on to evaluate.
struct Compiled {
    program: slc_syntax::ast::Program,
    traits: slc_syntax::traits::TraitInfo,
    defs: Vec<(String, slc_core::term::Term)>,
}

fn run_file(path: &std::path::Path, fuel: usize) -> Result<RunOutcome, String> {
    let compile_span = slc_core::span!("compile");
    let compile_guard = compile_span.enter();
    let Compiled { program, traits, defs } =
        compile_file(path).map_err(|diagnostics| diagnostics.join("\n"))?;
    // The whole program compiles to one flat chunk: every definition's
    // closures index it, so they must share it, and it stays installed for
    // the setup and the run.
    let (chunk, roots) = slc_runtime::compile::compile_program(&defs);
    drop(compile_guard);
    drop(compile_span);

    validate_main(&program)?;
    let main_root = roots
        .iter()
        .find(|(name, _)| name == "main")
        .map(|(_, root)| *root)
        .ok_or("no `main`: define `proc main | (exit: -i32) / {IO} { ... }`")?;
    let eval_span = slc_core::span!("eval");
    let _eval_guard = eval_span.enter();

    slc_runtime::chunk::with_chunk(chunk, || {
        run_program(&program, &traits, &roots, main_root, fuel)
    })
}

/// The compiler's phases, in the order `docs/design/programs.md` gives them:
/// parse, type, polarity, exhaustiveness, lowering. Each stops the ones after
/// it.
fn compile_file(path: &std::path::Path) -> Result<Compiled, Diagnostics> {
    let (map, tokens) = load_program(path)?;
    let format_span = |span| map.locate(span);
    let program = parse_units(&map, tokens)?;

    // The program's own definitions shadow the prelude's: globals install
    // in declaration order with the last one winning, and the prelude is
    // appended, so a redeclared name keeps only its first — the user's —
    // declaration.
    let program = shadow_prelude(program);

    // Modules flatten into qualified names before anything else looks.
    let program = slc_syntax::resolve::resolve_program_split(&program, &map.boundaries()).map_err(
        |errors| {
            errors
                .iter()
                .map(|e| format!("resolve: {} (at {})", e.message, format_span(e.span)))
                .collect::<Vec<_>>()
        },
    )?;

    // Traits elaborate away: impls become mangled functions, and a registry
    // records method signatures and per-type impls.
    let (program, traits) = slc_syntax::traits::elaborate(&program).map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("trait: {} (at {})", e.message, format_span(e.span)))
            .collect::<Vec<_>>()
    })?;

    // Type, polarity, and exhaustiveness checking. Checking also
    // resolves each monomorphic trait-method call to its impl, for static
    // dispatch in lowering.
    let (resolved, row_diagnostics) = slc_check::expr::check_program_with_rows(&program, &traits)
        .map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("type: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>()
    })?;
    slc_check::polarity::check_program(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("polarity: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>()
    })?;
    slc_check::exhaustive::check_exhaustiveness(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("exhaustiveness: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>()
    })?;

    // Effects are rows in the types just checked: what they refuse is
    // reported once the other checks have passed.
    if !row_diagnostics.is_empty() {
        return Err(row_diagnostics
            .iter()
            .map(|d| format!("effect: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>());
    }

    let defs = slc_syntax::lower::lower_program_resolving(&program, &resolved)
        .map_err(|e| vec![format!("lowering: {e}")])?;
    Ok(Compiled { program, traits, defs })
}

/// Run a compiled program: install its globals, then run `main` through its
/// exit continuation. Runs with the program's chunk already installed. The
/// globals' setup and the run share one budget of `fuel` machine steps.
fn run_program(
    program: &slc_syntax::ast::Program,
    traits: &slc_syntax::traits::TraitInfo,
    roots: &[(String, slc_runtime::chunk::NodeId)],
    main_root: slc_runtime::chunk::NodeId,
    mut fuel: usize,
) -> Result<RunOutcome, String> {
    let mut env = slc_runtime::value::Env::new();
    slc_runtime::value::install_stdlib(&mut env);
    // Functions are installed into the shared globals frame, so closures
    // resolve each other at call time regardless of definition order.
    // Enum constructors are also injected as string-valued globals.
    for d in &program.decls {
        if let slc_syntax::ast::Decl::Effect { name, operations, .. } = &d.kind {
            for op in operations {
                env.define_global(
                    op.name.clone(),
                    slc_runtime::value::Value::Operation {
                        effect: name.clone(),
                        op: op.name.clone(),
                    },
                );
            }
        }
        if let slc_syntax::ast::Decl::Enum { name, variants, .. } = &d.kind {
            for (v, _) in variants {
                env.define_global(
                    format!("{name}::{v}"),
                    slc_runtime::value::Value::Tagged(
                        format!("{name}::{v}"),
                        Box::new(slc_runtime::value::Value::Unit),
                    ),
                );
            }
        }
    }
    for (name, root) in roots {
        if name == "main" {
            continue;
        }
        let v =
            slc_runtime::eval::run_node(*root, &mut env, &mut fuel).map_err(|e| e.to_string())?;
        env.define_global(name, v);
    }
    // Trait methods need no runtime method value: every accepted call was
    // resolved by the checker to a direct impl call (concrete receiver) or a
    // dictionary projection (bounded receiver).
    //
    // Build a dictionary per `(trait, type)` with an impl: the trait's method
    // impls, in declaration order, as one value — a tuple, or
    // the lone impl for a single-method trait. A bounded function receives
    // one and projects its methods; monomorphic calls dispatch directly and
    // never consult these.
    for (trait_name, methods) in &traits.traits {
        let keys: Vec<String> = methods
            .first()
            .and_then(|m| traits.method_impls.get(&m.name))
            .map(|per_type| per_type.keys().cloned().collect())
            .unwrap_or_default();
        for key in keys {
            let impls: Vec<_> = methods
                .iter()
                .filter_map(|m| traits.method_impls.get(&m.name).and_then(|t| t.get(&key)))
                .filter_map(|mangled| env.lookup(mangled))
                .collect();
            if impls.len() != methods.len() {
                continue; // an incomplete impl — leave the dictionary unbuilt
            }
            let dict = match <[_; 1]>::try_from(impls) {
                Ok([lone]) => lone,
                Err(impls) if impls.is_empty() => continue,
                Err(impls) => slc_runtime::value::Value::Tuple(impls),
            };
            env.define_global(slc_syntax::lower::dict_global_name(trait_name, &key), dict);
        }
    }

    // The program's exit continuation is `EXIT`: supplying it to `main` runs
    // the program, and the cut that reaches it is what ends it. `main` is a
    // command like any other, so it takes both groups — the empty value
    // group first, as the unit, then the menu of exits.
    let entry =
        slc_runtime::eval::run_node(main_root, &mut env, &mut fuel).map_err(|e| e.to_string())?;
    let entry = slc_runtime::eval::apply_value(entry, slc_runtime::value::Value::Unit, &mut fuel)
        .map_err(|e| e.to_string())?;
    match slc_runtime::eval::apply_under_io(
        entry,
        slc_runtime::value::Value::Builtin("EXIT".into()),
        &mut fuel,
    ) {
        Err(slc_runtime::eval::EvalError::Exit(code)) => Ok(RunOutcome::Exit(code)),
        Ok(value) => Err(format!(
            "`main` finished without leaving through its exit continuation, with {}",
            value.display()
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// Whether the program declares a `main` at all: a file that does not is a
/// library, which `check` accepts and `run` cannot start.
fn declares_main(program: &slc_syntax::ast::Program) -> bool {
    use slc_syntax::ast::Decl;
    program.decls.iter().any(|decl| {
        matches!(&decl.kind, Decl::Command { name, .. } | Decl::Fn { name, .. } if name == "main")
    })
}

/// A program is a command, so its entry point is a `command`: it takes no values
/// and exactly one continuation — the exit status — and every terminating
/// path leaves through it.
fn validate_main(program: &slc_syntax::ast::Program) -> Result<(), String> {
    use slc_syntax::ast::{Decl, TypeExpr};
    let mut mains = program
        .decls
        .iter()
        .filter(|decl| matches!(&decl.kind, Decl::Command { name, .. } | Decl::Fn { name, .. } if name == "main"));
    let Some(main) = mains.next() else {
        return Err("no `main`: define `proc main | (exit: -i32) / {IO} { ... }`".into());
    };
    if mains.next().is_some() {
        return Err("program contains multiple `main` declarations".into());
    }
    let Decl::Command { value_params, continuation_params, .. } = &main.kind else {
        return Err(MAIN_ENTRY_POINT_ERROR.into());
    };
    let [exit] = continuation_params.as_slice() else {
        return Err(MAIN_ENTRY_POINT_ERROR.into());
    };
    let exits_with_a_status = matches!(&exit.ty, Some(TypeExpr::Negative(inner))
        if matches!(&inner.kind, TypeExpr::Base(name) if name == "i32"));
    if !value_params.is_empty() || !exits_with_a_status {
        return Err(MAIN_ENTRY_POINT_ERROR.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::library_for;

    fn loaded(program: &str) -> Vec<&'static str> {
        library_for(program).into_iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn a_program_that_reaches_no_module_loads_only_the_prelude() {
        assert_eq!(
            loaded(r#"proc main | (exit: i32) / {IO} { <"hi" | println; <0 | exit> }"#),
            ["prelude"]
        );
    }

    #[test]
    fn a_module_loads_with_the_modules_it_reaches() {
        // A path is enough; so is a `use`, of a name or a glob.
        assert_eq!(loaded("x | fs::read"), ["prelude", "fs"]);
        assert_eq!(loaded("use num::*;"), ["prelude", "num"]);
        assert_eq!(loaded("use option;"), ["prelude", "option"]);
        // `seq` reaches `list` and `stream`, and `stream` reaches `list`.
        assert_eq!(loaded("use seq::Seq;"), ["prelude", "list", "stream", "seq"]);
    }
}
