use std::path::PathBuf;
use std::process::ExitCode;

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

    match run_file(&file) {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            if let Some(code) = e.strip_prefix("exit(").and_then(|c| c.strip_suffix(")")) {
                let code: i32 = code.parse().unwrap_or(1);
                return if code == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE };
            }
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_file(path: &PathBuf) -> Result<String, String> {
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
        diags.iter().map(|d| format!("polarity: {}", d.message)).collect::<Vec<_>>().join("\n")
    })?;
    slc_check::linearity::check_linearity(&program).map_err(|diags| {
        diags.iter().map(|d| format!("linearity: {}", d.message)).collect::<Vec<_>>().join("\n")
    })?;
    slc_check::exhaustive::check_exhaustiveness(&program).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("exhaustiveness: {}", d.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let defs = slc_syntax::lower::lower_program(&program).map_err(|e| format!("lowering: {e}"))?;
    drop(_compile_guard);
    drop(compile_span);

    // Find main and evaluate
    let main = defs.iter().find(|(name, _)| name == "main").ok_or("no `main` function")?;
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
                    format!("{name}_{v}"),
                    slc_runtime::value::Value::Str(format!("{name}::{v}")),
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
    let mut fuel = 1_000_000;
    let value = slc_runtime::eval::eval(&main.1, &mut env, &mut fuel).map_err(|e| e.to_string())?;

    Ok(value.display())
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
