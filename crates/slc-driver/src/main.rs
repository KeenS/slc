use std::path::PathBuf;
use std::process::ExitCode;

enum RunOutcome {
    Exit(i32),
}

/// The prelude: ordinary declarations every program sees. See
/// `prelude.sl` for what belongs there.
const PRELUDE: &str = include_str!("prelude.sl");

const MAIN_ENTRY_POINT_ERROR: &str = "entry point must be `command main | (exit: -i32) { ... }`: a command with no value \
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
fn shadow_prelude(mut program: slc_syntax::ast::Program) -> slc_syntax::ast::Program {
    use slc_syntax::ast::Decl;
    let mut values = std::collections::HashSet::new();
    let mut types = std::collections::HashSet::new();
    program.decls.retain(|d| match &d.kind {
        Decl::Fn { name, .. } | Decl::Command { name, .. } | Decl::Const { name, .. } => {
            values.insert(name.clone())
        }
        Decl::Data { name, .. }
        | Decl::Enum { name, .. }
        | Decl::Menu { name, .. }
        | Decl::Form { name, .. } => types.insert(name.clone()),
        _ => true,
    });
    program
}

fn run_file(path: &PathBuf) -> Result<RunOutcome, String> {
    let compile_span = slc_core::span!("compile");
    let _compile_guard = compile_span.enter();
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    // The prelude is ordinary Slant source, appended after the program so
    // the program's spans — and so its diagnostics' line numbers — are
    // untouched. Everything in it goes through the same pipeline as user
    // code, and diagnostics that do point into the prelude still render,
    // since `source` is the combined text. The boundary scopes variant
    // imports to their own unit.
    // Spans count characters, not bytes — the surface has multi-byte
    // glyphs — so the boundary does too.
    let prelude_from = source.chars().count() + 1;
    let source = format!("{source}\n{PRELUDE}");

    let tokens = slc_syntax::lexer::lex(&source).map_err(|e| e.message)?;
    let program = slc_syntax::parser::parse(tokens).map_err(|errors| {
        errors.iter().map(|e| format!("parse error: {}", e.message)).collect::<Vec<_>>().join("\n")
    })?;

    // The program's own definitions shadow the prelude's: globals install
    // in declaration order with the last one winning, and the prelude is
    // appended, so a redeclared name keeps only its first — the user's —
    // declaration.
    let program = shadow_prelude(program);

    // Modules flatten into qualified names before anything else looks.
    let program =
        slc_syntax::resolve::resolve_program_split(&program, prelude_from).map_err(|errors| {
            errors
                .iter()
                .map(|e| format!("resolve: {} (at {})", e.message, format_span(&source, e.span)))
                .collect::<Vec<_>>()
                .join("\n")
        })?;

    // Traits elaborate away: impls become mangled functions, and a registry
    // records method signatures and per-type impls.
    let (program, traits) = slc_syntax::traits::elaborate(&program).map_err(|errors| {
        errors
            .iter()
            .map(|e| format!("trait: {} (at {})", e.message, format_span(&source, e.span)))
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
                .map(|d| format!("type: {} (at {})", d.message, format_span(&source, d.span)))
                .collect::<Vec<_>>()
                .join("\n")
        })?;
    slc_check::polarity::check_program(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("polarity: {} (at {})", d.message, format_span(&source, d.span)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    slc_check::exhaustive::check_exhaustiveness(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("exhaustiveness: {} (at {})", d.message, format_span(&source, d.span)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    slc_check::effects::check_effects(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("effect: {} (at {})", d.message, format_span(&source, d.span)))
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
        .ok_or("no `main`: define `command main | (exit: -i32) { ... }`")?;
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
        if let slc_syntax::ast::Decl::Effect { name, operations } = &d.kind {
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
    // impls, in declaration order, as one value — a right-nested tuple, or
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
            let mut it = impls.into_iter().rev();
            let Some(mut dict) = it.next() else { continue };
            for impl_value in it {
                dict = slc_runtime::value::Value::Pair(Box::new(impl_value), Box::new(dict));
            }
            env.define_global(slc_syntax::lower::dict_global_name(trait_name, &key), dict);
        }
    }

    // The program's exit continuation is `EXIT`: supplying it to `main` runs
    // the program, and the cut that reaches it is what ends it.
    let mut fuel = 1_000_000;
    let entry =
        slc_runtime::eval::run_node(main_root, &mut env, &mut fuel).map_err(|e| e.to_string())?;
    match slc_runtime::eval::apply_value(
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
        return Err("no `main`: define `command main | (exit: -i32) { ... }`".into());
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

fn format_span(source: &str, span: slc_syntax::token::Span) -> String {
    // Spans are byte offsets, and the surface has multi-byte glyphs — `⊗`,
    // `⊗`, `⊥` — so an offset may land inside one. Slicing there panics, so
    // move to the boundary rather than trusting the offset.
    let start = char_boundary(source, span.start, false);
    let end = char_boundary(source, span.end.max(span.start), true);
    let before = &source[..start];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rfind('\n')
        .map(|i| before[i..].chars().count())
        .unwrap_or(before.chars().count() + 1);
    format!("{line}:{column} `{}`", &source[start..end])
}

/// The nearest char boundary at or before `offset` (or after it, when
/// `forward`), clamped to the source.
fn char_boundary(source: &str, offset: usize, forward: bool) -> usize {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        if forward {
            offset += 1;
        } else {
            offset -= 1;
        }
    }
    offset
}
