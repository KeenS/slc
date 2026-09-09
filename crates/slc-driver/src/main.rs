use std::path::PathBuf;
use std::process::ExitCode;

enum RunOutcome {
    Exit(i32),
}

const MAIN_ENTRY_POINT_ERROR: &str = "entry point must be `mu main | (exit: -i32) { ... }`: a command with no value \
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

fn run_file(path: &PathBuf) -> Result<RunOutcome, String> {
    let compile_span = slc_core::span!("compile");
    let _compile_guard = compile_span.enter();
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    let tokens = slc_syntax::lexer::lex(&source).map_err(|e| e.message)?;
    let program = slc_syntax::parser::parse(tokens).map_err(|errors| {
        errors.iter().map(|e| format!("parse error: {}", e.message)).collect::<Vec<_>>().join("\n")
    })?;

    // Type, polarity, linearity, and exhaustiveness checking
    slc_check::expr::check_program(&program).map_err(|diags| {
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
    slc_check::linearity::check_linearity(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("linearity: {} (at {})", d.message, format_span(&source, d.span)))
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

    let defs = slc_syntax::lower::lower_program(&program).map_err(|e| format!("lowering: {e}"))?;
    drop(_compile_guard);
    drop(compile_span);

    validate_main(&program)?;
    let main = defs
        .iter()
        .find(|(name, _)| name == "main")
        .ok_or("no `main`: define `mu main | (exit: -i32) { ... }`")?;
    let eval_span = slc_core::span!("eval");
    let _eval_guard = eval_span.enter();

    let mut env = slc_runtime::value::Env::new();
    slc_runtime::value::install_stdlib(&mut env);
    // Functions are installed into the shared globals frame, so closures
    // resolve each other at call time regardless of definition order.
    // Enum constructors are also injected as string-valued globals.
    for d in &program.decls {
        if let slc_syntax::ast::Decl::Enum { name, variants } = &d.kind {
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
    for (name, term) in &defs {
        if name == "main" {
            continue;
        }
        let mut fuel = 1_000_000;
        let v = slc_runtime::eval::eval(term, &mut env, &mut fuel).map_err(|e| e.to_string())?;
        env.define_global(name, v);
    }
    // The program's exit continuation is `EXIT`: supplying it to `main` runs
    // the program, and the cut that reaches it is what ends it.
    let mut fuel = 1_000_000;
    let entry = slc_runtime::eval::eval(&main.1, &mut env, &mut fuel).map_err(|e| e.to_string())?;
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
        return Err("no `main`: define `mu main | (exit: -i32) { ... }`".into());
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
    let before = &source[..span.start.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rfind('\n').map(|i| span.start - i).unwrap_or(span.start + 1);
    format!(
        "{line}:{column} `{}`",
        &source[span.start.min(source.len())..span.end.min(source.len())]
    )
}
