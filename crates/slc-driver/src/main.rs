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
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_file(path: &PathBuf) -> Result<String, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    let tokens = slc_syntax::lexer::lex(&source).map_err(|e| e.message)?;
    let program = slc_syntax::parser::parse(tokens).map_err(|errors| {
        errors.iter().map(|e| format!("parse error: {}", e.message)).collect::<Vec<_>>().join("\n")
    })?;

    // Polarity and linearity checking
    slc_check::polarity::check_program(&program).map_err(|diags| {
        diags.iter().map(|d| format!("polarity: {}", d.message)).collect::<Vec<_>>().join("\n")
    })?;
    slc_check::linearity::check_linearity(&program).map_err(|diags| {
        diags.iter().map(|d| format!("linearity: {}", d.message)).collect::<Vec<_>>().join("\n")
    })?;

    let defs = slc_syntax::lower::lower_program(&program).map_err(|e| format!("lowering: {e}"))?;

    // Find main and evaluate
    let main = defs.iter().find(|(name, _)| name == "main").ok_or("no `main` function")?;

    let mut env = slc_runtime::value::Env::new();
    slc_runtime::value::install_stdlib(&mut env);
    // Functions are installed into the shared globals frame, so closures
    // resolve each other at call time regardless of definition order.
    for (name, term) in &defs {
        if name == "main" {
            continue;
        }
        let mut fuel = 1_000_000;
        let v =
            slc_runtime::eval::eval(term, &mut env, &mut fuel).map_err(|e| format!("eval: {e}"))?;
        env.define_global(name, v);
    }
    let mut fuel = 1_000_000;
    let value =
        slc_runtime::eval::eval(&main.1, &mut env, &mut fuel).map_err(|e| format!("eval: {e}"))?;

    Ok(value.display())
}
