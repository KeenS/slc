use std::path::PathBuf;
use std::process::ExitCode;

enum RunOutcome {
    Exit(i32),
}

/// The library, as source units appended after the program: the prelude
/// first — ordinary declarations every program sees unasked — then each
/// stdlib module, which a program reaches only through `use`. Every unit
/// goes through the same pipeline as user code. See `prelude.sl` and
/// `stdlib/` for what belongs where.
const LIBRARY: &[(&str, &str)] = &[
    ("prelude", include_str!("prelude.sl")),
    ("num", include_str!("stdlib/num.sl")),
    ("list", include_str!("stdlib/list.sl")),
    ("option", include_str!("stdlib/option.sl")),
    ("either", include_str!("stdlib/either.sl")),
    ("lazy", include_str!("stdlib/lazy.sl")),
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
struct SourceMap {
    text: String,
    /// `(name, char offset of the unit's first char)`, in order.
    units: Vec<(String, usize)>,
}

impl SourceMap {
    /// The program's text under `program`, then each library unit it
    /// needs, each on a line of its own. The program comes first so its
    /// spans — and its diagnostics' line numbers — are untouched.
    fn new(program: &str, source: String) -> Self {
        let library = library_for(&source);
        let mut text = source;
        let mut units = vec![(program.to_string(), 0)];
        for (name, unit) in library {
            text.push('\n');
            units.push((name.to_string(), text.chars().count()));
            text.push_str(unit);
        }
        SourceMap { text, units }
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
        if unit == self.units[0].0 {
            format!("{line}:{column} `{snippet}`")
        } else {
            format!("{unit}.sl:{line}:{column} `{snippet}`")
        }
    }
}

const MAIN_ENTRY_POINT_ERROR: &str = "entry point must be `command main | (exit: -i32) / {IO} { ... }`: a command with no value \
     parameters and one continuation, the exit status";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version") {
        println!("slc {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let file = match args.iter().position(|a| a == "run") {
        Some(i) if args.len() > i + 1 => PathBuf::from(&args[i + 1]),
        _ => {
            eprintln!("usage: slc run <file.sl>");
            return ExitCode::FAILURE;
        }
    };

    // A continuation-passing program nests as deeply as its control flow,
    // and the evaluator walks the tree on the host stack, so give it room.
    let outcome = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run_file(&file))
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

fn run_file(path: &PathBuf) -> Result<RunOutcome, String> {
    let compile_span = slc_core::span!("compile");
    let _compile_guard = compile_span.enter();
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let map = SourceMap::new(&path.display().to_string(), source);
    let source = &map.text;
    let format_span = |span| map.locate(span);

    let tokens = slc_syntax::lexer::lex(source).map_err(|e| e.message)?;
    let program = slc_syntax::parser::parse(tokens).map_err(|errors| {
        errors.iter().map(|e| format!("parse error: {}", e.message)).collect::<Vec<_>>().join("\n")
    })?;

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
                .join("\n")
        },
    )?;

    // Traits elaborate away: impls become mangled functions, and a registry
    // records method signatures and per-type impls.
    let (program, traits) = slc_syntax::traits::elaborate(&program).map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("trait: {} (at {})", e.message, format_span(e.span)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    // Type, polarity, and exhaustiveness checking. Checking also
    // resolves each monomorphic trait-method call to its impl, for static
    // dispatch in lowering.
    let resolved =
        slc_check::expr::check_program_resolving(&program, &traits).map_err(|diags| {
            diags
                .iter()
                .map(|d| format!("type: {} (at {})", d.message, format_span(d.span)))
                .collect::<Vec<_>>()
                .join("\n")
        })?;
    slc_check::polarity::check_program(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("polarity: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    slc_check::exhaustive::check_exhaustiveness(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("exhaustiveness: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    slc_check::effects::check_effects(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("effect: {} (at {})", d.message, format_span(d.span)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let defs = slc_syntax::lower::lower_program_resolving(&program, &resolved)
        .map_err(|e| format!("lowering: {e}"))?;
    // The whole program compiles to one flat chunk: every definition's
    // closures index it, so they must share it, and it stays installed for
    // the setup and the run.
    let (chunk, roots) = slc_runtime::compile::compile_program(&defs);
    drop(_compile_guard);
    drop(compile_span);

    validate_main(&program)?;
    let main_root = roots
        .iter()
        .find(|(name, _)| name == "main")
        .map(|(_, root)| *root)
        .ok_or("no `main`: define `command main | (exit: -i32) / {IO} { ... }`")?;
    let eval_span = slc_core::span!("eval");
    let _eval_guard = eval_span.enter();

    slc_runtime::chunk::with_chunk(chunk, || run_program(&program, &traits, &roots, main_root))
}

/// Run a compiled program: install its globals, then run `main` through its
/// exit continuation. Runs with the program's chunk already installed.
fn run_program(
    program: &slc_syntax::ast::Program,
    traits: &slc_syntax::traits::TraitInfo,
    roots: &[(String, slc_runtime::chunk::NodeId)],
    main_root: slc_runtime::chunk::NodeId,
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
        let mut fuel = 1_000_000;
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
    let mut fuel = 1_000_000;
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
        return Err("no `main`: define `command main | (exit: -i32) / {IO} { ... }`".into());
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
            loaded(r#"command main | (exit: i32) / {IO} { "hi" | println; 0 | exit⟩ }"#),
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
