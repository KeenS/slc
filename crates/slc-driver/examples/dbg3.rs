fn main() {
    let src = r#"
fn skip2(input: String, pos: i64) -> i64 { pos }
fn main() -> i32 { skip2("ab", 0) }
"#;
    let toks = slc_syntax::lexer::lex(src).unwrap();
    let prog = slc_syntax::parser::parse(toks).unwrap();
    let defs = slc_syntax::lower::lower_program(&prog).unwrap();
    let mut env = slc_runtime::value::Env::new();
    slc_runtime::value::install_stdlib(&mut env);
    for (name, term) in &defs {
        if name != "main" {
            let mut fuel = 1000;
            let v = slc_runtime::eval::eval(term, &mut env, &mut fuel).unwrap();
            env.define(name, v);
        }
    }
    let main = defs.iter().find(|(n, _)| *n == "main").unwrap();
    let mut fuel = 100000;
    match slc_runtime::eval::eval(&main.1, &mut env, &mut fuel) {
        Ok(v) => println!("OK: {}", v.display()),
        Err(e) => println!("ERR: {e}"),
    }
}
