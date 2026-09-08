fn main() {
    let src = r#"
fn parse_json(input: String) -> i64 {
    let start = skip_ws(input, 0);
    start
}
fn main() -> i32 { parse_json(" 42") }
"#;
    let toks = slc_syntax::lexer::lex(src).unwrap();
    let prog = slc_syntax::parser::parse(toks).unwrap();
    let defs = slc_syntax::lower::lower_program(&prog).unwrap();
    for (n, t) in &defs {
        println!("{n}: {t}");
        println!();
    }
}
