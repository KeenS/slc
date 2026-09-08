fn main() {
    let src = r#"
fn parse_value(input: String, pos: i64) -> i64 {
    let ch = char_at(input, pos);
    ch
}
fn main() -> i32 { parse_value("42", 0) }
"#;
    let toks = slc_syntax::lexer::lex(src).unwrap();
    let prog = slc_syntax::parser::parse(toks).unwrap();
    let defs = slc_syntax::lower::lower_program(&prog).unwrap();
    for (n, t) in &defs {
        println!("{n}: {t}");
        println!();
    }
}
