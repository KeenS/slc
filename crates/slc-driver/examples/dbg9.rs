fn main() {
    let src = r#"
fn parse_number(input: String, pos: i64) -> i64 {
    let start = pos;
    let end = skip_digits(input, pos);
    str_to_int(substring(input, start, end))
}
"#;
    let toks = slc_syntax::lexer::lex(src).unwrap();
    let prog = slc_syntax::parser::parse(toks).unwrap();
    let defs = slc_syntax::lower::lower_program(&prog).unwrap();
    for (n, t) in &defs {
        println!("{n}: {t}\n");
    }
}
