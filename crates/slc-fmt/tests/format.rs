//! What `slc fmt` writes, rule by rule, and that it is safe: formatting is
//! idempotent, and every `.sl` file in the repository is already formatted.

use slc_fmt::{FormatError, format_source};

/// Format, and check that formatting the result changes nothing.
fn fmt(source: &str) -> String {
    let once = format_source(source).unwrap_or_else(|e| panic!("{e}\n{source}"));
    let twice = format_source(&once).unwrap_or_else(|e| panic!("{e}\n{once}"));
    assert_eq!(once, twice, "formatting is not idempotent");
    once
}

#[test]
fn spacing_is_normalised() {
    assert_eq!(
        fmt("func  add( x:i64 ,y :i64 )->i64{<( x,y )|__add}"),
        "func add(x: i64, y: i64) -> i64 { <(x, y) | __add }\n"
    );
    assert_eq!(
        fmt("proc main|(exit:i32)/{IO}{\n<0|exit>}"),
        "proc main | (exit: i32) / {IO} {\n    <0 | exit>\n}\n"
    );
    assert_eq!(
        fmt("pub func map<+A,+B,E>(f:(A->B/{..E}),xs:List<A>)->List<B>/{..E}{ xs }"),
        "pub func map<+A, +B, E>(f: (A -> B / {..E}), xs: List<A>) -> List<B> / {..E} { xs }\n"
    );
}

#[test]
fn a_negative_function_keeps_its_arrow_and_its_menu_of_exits() {
    assert_eq!(
        fmt("func run(found:i64&missing:String)<-i64{ found }"),
        "func run(found: i64 & missing: String) <- i64 { found }\n"
    );
    assert_eq!(fmt("data Box<-T>{inner:T}"), "data Box<-T> { inner: T }\n");
}

#[test]
fn a_module_in_a_file_of_its_own_is_one_line() {
    assert_eq!(
        fmt("pub  sect geometry ;\nsect outer{sect inner;}"),
        "pub sect geometry;\nsect outer { sect inner; }\n"
    );
}

#[test]
fn several_bounds_are_joined_by_spaced_pluses() {
    assert_eq!(
        fmt("func f<+T:Ord+Into<i64>,-K:Show>(x:T)->T{x}\nimpl<+T:Ord+Loud>Into<T> for Pair<T>{}"),
        "func f<+T: Ord + Into<i64>, -K: Show>(x: T) -> T { x }\nimpl<+T: Ord + Loud> Into<T> for Pair<T> {}\n"
    );
}

#[test]
fn statements_end_in_semicolons_and_the_last_expression_does_not() {
    let expected = "func f() -> i64 {\n    let x = 1;\n    x | println;\n    x\n}\n";
    assert_eq!(fmt("func f() -> i64 { let x = 1 x | println; x; }"), expected);
    assert_eq!(fmt(expected), expected);
    // A `let` ends in `;` even when it ends the block.
    assert_eq!(fmt("func f() -> i64 { let x = 1 }"), "func f() -> i64 {\n    let x = 1;\n}\n");
}

#[test]
fn a_brace_list_keeps_the_line_break_its_author_gave_it() {
    let inline = "func not(b: Bool) -> Bool { of b { True => False, False => True } }\n";
    assert_eq!(fmt(inline), inline);
    let broken = "func not(b: Bool) -> Bool {\n    of b {\n        True => False,\n        False => True,\n    }\n}\n";
    assert_eq!(fmt("func not(b: Bool) -> Bool { of b {\nTrue => False, False => True } }"), broken);
    assert_eq!(fmt("enum Bool{False,True}"), "enum Bool { False, True }\n");
    assert_eq!(fmt("enum Bool{\nFalse,True}"), "enum Bool {\n    False,\n    True,\n}\n");
}

#[test]
fn arms_get_commas_and_a_broken_list_a_trailing_one() {
    assert_eq!(
        fmt("func f(b: Bool) -> i64 { of b {\n True => 1 False => 2 } }"),
        "func f(b: Bool) -> i64 {\n    of b {\n        True => 1,\n        False => 2,\n    }\n}\n"
    );
    assert_eq!(
        fmt("func f() -> i64 { do g() hn { ask(x): k => <x | k, return(v) => v, } }"),
        "func f() -> i64 { do g() hn { ask(x): k => <x | k, return(v) => v } }\n"
    );
}

#[test]
fn a_list_too_long_for_its_line_breaks_one_element_per_line() {
    let source = "func f() -> i64 { let more = of (<(pos, <input | str_len) | lt) { True => <(input, pos) | at | is_digit, False => False }; more }";
    assert_eq!(
        fmt(source),
        "func f() -> i64 {\n    let more = of (<(pos, <input | str_len) | lt) {\n        True => <(input, pos) | at | is_digit,\n        False => False,\n    };\n    more\n}\n"
    );
}

#[test]
fn a_long_header_breaks_at_its_parameters_first() {
    let source = "pub func take_while<+T, E>(keep: (T -> Bool / {..E}), s: (-> Stream<T, ..E> / {..E})) -> Seq<T, ..E> { s }";
    assert_eq!(
        fmt(source),
        "pub func take_while<+T, E>(\n    keep: (T -> Bool / {..E}),\n    s: (-> Stream<T, ..E> / {..E}),\n) -> Seq<T, ..E> { s }\n"
    );
    let source = "pub proc nth<+T, E>(xs: List<T>, i: i64) | (found: (-T / {..E}) & missing: (-String / {..E})) / {..E} { <i | found> }";
    assert_eq!(
        fmt(source),
        "pub proc nth<+T, E>(xs: List<T>, i: i64) | (\n    found: (-T / {..E})\n    & missing: (-String / {..E})\n) / {..E} { <i | found> }\n"
    );
}

#[test]
fn a_long_chain_breaks_before_each_pipe_and_a_binder_keeps_its_next_stage() {
    let source = r#"func f(text: String) -> (,) / {IO} { <("about to write ", <text | str_len | to_string) | add | x => (x, " characters") | add | println }"#;
    assert_eq!(
        fmt(source),
        "func f(text: String) -> (,) / {IO} {\n    <(\"about to write \", <text | str_len | to_string)\n        | add\n        | x => (x, \" characters\") | add\n        | println\n}\n"
    );
}

#[test]
fn a_chain_hugs_the_block_it_ends_in() {
    let source = "proc main | (exit: i32) / {IO} {\n    <Shape::Circle(5) | area_of | label_of | mu String {\n        answer => <answer | println,\n    }>\n}\n";
    assert_eq!(fmt(source), source);
    let source = "proc main | (exit: i32) / {IO} {\n    <do (<6 | emit) hn {\n        config(): resume => <7 | resume,\n    } | println;\n    <0 | exit>\n}\n";
    assert_eq!(fmt(source), source);
}

#[test]
fn touching_signs_stay_touching() {
    // `<-1` is a chain opening on a negative number, `let+` a mode, and
    // `let -1` a negative literal pattern: spacing is syntax in all three.
    assert_eq!(fmt("func f(k: -i64) -> (;) { <-1|k> }"), "func f(k: -i64) -> (;) { <-1 | k> }\n");
    let expected =
        "func f() -> i64 {\n    let+ x = g();\n    let- y = g();\n    let -1 = x;\n    y\n}\n";
    assert_eq!(fmt("func f() -> i64 { let+ x = g(); let- y = g(); let -1 = x; y }"), expected);
}

#[test]
fn literals_are_kept_as_written() {
    let source =
        "def BIG: i64 = 1_000_000;\ndef QUOTE: char = '\\'';\ndef TEXT: String = \"a\\tb\\\\\";\n";
    assert_eq!(fmt(source), source);
}

#[test]
fn parentheses_are_never_added_or_dropped() {
    assert_eq!(fmt("func f(x: i64) -> i64 { ( (x) ,) }"), "func f(x: i64) -> i64 { ((x),) }\n");
    assert_eq!(fmt("func f() -> (&) { ( & ) }"), "func f() -> (&) { (&) }\n");
    assert_eq!(fmt("func f() -> (,) { ( , ) }"), "func f() -> (,) { (,) }\n");
}

#[test]
fn a_scrutinee_is_not_a_record_literal() {
    let source = "func f(p: P) -> i64 { of p { P { x, y: _ } => x } }\n";
    assert_eq!(fmt(source), source);
    let source =
        "func f(x: i64) -> P { P { x: x, y: of x { 0 => Q { z: 1 }, _ => Q { z: 2 } } } }\n";
    assert_eq!(fmt(source), source);
}

#[test]
fn every_declaration_form_is_laid_out() {
    let source = "\
cite list::List::{Nil, Cons};
cite list::*;

hook Reader<+T> { func ask() -> T; }

spec Show {
    func show(self: Self) -> String;
    proc emit(self: Self) | (out: String);
}

spec Into<+U> {
    func into(self: Self) -> U;
}

impl<+T: Show> Show for List<T> {
    func show(self: List<T>) -> String { \"list\" }
}

menu Seq<+T, E> / {..E} { next: Step<T, ..E> }

form Report / {IO} { value: i64, label: String }

sect m {
    pub def N: i64 = 1;
}
";
    assert_eq!(fmt(source), source);
    assert_eq!(
        fmt("cite list::List::{Nil,Cons}\ndef N:i64=1"),
        "cite list::List::{Nil, Cons};\ndef N: i64 = 1;\n"
    );
}

#[test]
fn every_expression_form_is_laid_out() {
    let source = "\
func f(k: -Config, p: (i64 | String)) -> i64 {
    let a = mu i64 { out <= <1 | out> };
    let b = mu Config { retries <= <3 | retries>, name: out <= <\"n\" | out> };
    let c = of k { .retries(out) <= .retries(out), .name(out) <= .name(out) };
    let d = of p { ::0(n) => n, ::1(_) => 0 };
    let e = of a { 0..=9 | 10 => 1, n @ 11 => n, -1 => 2, _ => 3 };
    let g = hn [Reader, State] { ask(): k => <1 | k, _ => forward };
    let h = do (reset ask()) g;
    let i = fn(x: i64) -> i64 { x };
    let j = fn { 1 };
    let l = (a ; b);
    let m = double | incr | k>;
    <(a, b.0, c.name) | list::sum
}
";
    assert_eq!(fmt(source), source);
}

#[test]
fn comments_keep_their_place() {
    let source = "\
// A file comment.

// Above the function.
func f(x: i64) -> i64 {
    // Above the statement.
    let y = x; // beside it

    // Above the match, after a blank line.
    of y {
        // Above an arm.
        0 => 1, // beside an arm
        _ => 2,
        // After the last arm.
    }
}

// After everything.
";
    assert_eq!(fmt(source), source);
}

#[test]
fn a_trailing_comment_is_one_space_from_its_line() {
    assert_eq!(
        fmt("func f() -> i64 {\n    <1 | println;              // one\n    2\n}\n"),
        "func f() -> i64 {\n    <1 | println; // one\n    2\n}\n"
    );
}

#[test]
fn a_comment_breaks_the_list_it_is_in() {
    assert_eq!(fmt("enum E { A, // first\n B }"), "enum E {\n    A, // first\n    B,\n}\n");
    assert_eq!(
        fmt("func f() -> i64 { g(1, /* two */ 2) }"),
        "func f() -> i64 { g(1, /* two */ 2) }\n"
    );
    assert_eq!(
        fmt("func f() -> i64 { <1\n // why\n | g }"),
        "func f() -> i64 {\n    <1\n        // why\n        | g\n}\n"
    );
    assert_eq!(
        fmt("func f() -> (,) {\n    // nothing yet\n}\n"),
        "func f() -> (,) {\n    // nothing yet\n}\n"
    );
}

#[test]
fn blank_lines_are_kept_but_never_doubled() {
    assert_eq!(
        fmt(
            "\n\nfunc f() -> i64 {\n\n    let x = 1;\n\n\n\n    x\n\n}\n\n\n\nfunc g() -> i64 { 1 }\nfunc h() -> i64 { 2 }\n\n"
        ),
        "func f() -> i64 {\n    let x = 1;\n\n    x\n}\n\nfunc g() -> i64 { 1 }\nfunc h() -> i64 { 2 }\n"
    );
}

#[test]
fn source_that_does_not_parse_is_refused_and_says_why() {
    let error = format_source("func f() -> i64 { a && b }").unwrap_err();
    assert!(
        matches!(&error, FormatError::Syntax { message, .. } if message.contains("&&")),
        "{error:?}"
    );
    assert!(matches!(format_source("func f() -> i64 { \"open }"), Err(FormatError::Syntax { .. })));
}

/// Every `.sl` file the repository ships: the examples, the prelude, the
/// standard library.
fn repository_sources() -> Vec<std::path::PathBuf> {
    fn collect(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("a source directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "sl") {
                files.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    collect(&root.join("examples"), &mut files);
    collect(&root.join("crates/slc-driver/src"), &mut files);
    assert!(files.len() > 50, "found only {} sources", files.len());
    files
}

#[test]
fn the_repository_sources_are_formatted() {
    let unformatted: Vec<String> = repository_sources()
        .into_iter()
        .filter(|path| {
            let source = std::fs::read_to_string(path).expect("a readable source");
            let formatted =
                format_source(&source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            formatted != source
        })
        .map(|path| path.display().to_string())
        .collect();
    assert!(unformatted.is_empty(), "run `slc fmt` on: {unformatted:#?}");
}
