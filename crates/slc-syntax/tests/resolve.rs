//! Modules flatten into qualified names, and nothing downstream knows they
//! existed.

use slc_syntax::ast::Decl;
use slc_syntax::lexer::lex;
use slc_syntax::parser::parse;
use slc_syntax::resolve::resolve_program;

fn resolved(source: &str) -> slc_syntax::ast::Program {
    resolve_program(&parse(lex(source).unwrap()).unwrap()).unwrap()
}

fn names(program: &slc_syntax::ast::Program) -> Vec<String> {
    program
        .decls
        .iter()
        .map(|d| match &d.kind {
            Decl::Fn { name, .. }
            | Decl::Command { name, .. }
            | Decl::Data { name, .. }
            | Decl::Enum { name, .. }
            | Decl::Const { name, .. } => name.clone(),
            _ => "UNRESOLVED".into(),
        })
        .collect()
}

#[test]
fn declarations_qualify_and_modules_disappear() {
    let p = resolved(
        "mod a {
             fn f() -> i64 { 1 }
             mod b { fn g() -> i64 { 2 } }
         }
         fn h() -> i64 { 3 }",
    );
    assert_eq!(names(&p), vec!["a::f", "a::b::g", "h"]);
}

#[test]
fn references_resolve_by_scope() {
    // In-module bare names, sibling paths, and the walk to an ancestor.
    let p = resolved(
        "mod outer {
             fn shared() -> i64 { 1 }
             mod inner { pub fn deep() -> i64 { shared() } }
             fn from_sibling() -> i64 { inner::deep() }
         }",
    );
    let bodies: Vec<String> = p.decls.iter().map(|d| format!("{:?}", d.kind)).collect();
    assert!(bodies[1].contains("\"outer::shared\""), "{:?}", bodies[1]);
    assert!(bodies[2].contains("\"outer::inner::deep\""), "{:?}", bodies[2]);
}

#[test]
fn a_private_declaration_is_its_module_s_own() {
    // Reachable inside the module, and from a module nested in it.
    assert!(
        resolve_program(
            &parse(
                lex("mod m { fn helper() -> i64 { 1 }
                           pub fn f() -> i64 { helper() }
                           mod deeper { pub fn g() -> i64 { helper() } } }")
                .unwrap()
            )
            .unwrap()
        )
        .is_ok()
    );
    // And nowhere else.
    let errors = resolve_program(
        &parse(
            lex("mod m { fn helper() -> i64 { 1 } }
                    fn caller() -> i64 { m::helper() }")
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("`m::helper` is private to `m`")),
        "{errors:?}"
    );
}

#[test]
fn a_use_gives_a_name_and_a_local_takes_it_back() {
    let p = resolved(
        "mod m { pub fn f() -> i64 { 1 } }
         use m::f;
         fn caller() -> i64 { f() }
         fn shadows() -> i64 { let f = 5; f }",
    );
    let caller = format!("{:?}", p.decls[1].kind);
    assert!(caller.contains("\"m::f\""), "{caller}");
    let shadows = format!("{:?}", p.decls[2].kind);
    assert!(!shadows.contains("m::f"), "a local binding shadows the use: {shadows}");
}

#[test]
fn an_unclaimed_name_is_left_for_later_passes() {
    // `println` is a builtin: no module claims it, so it stays bare.
    let p = resolved("mod m { fn f() -> i64 { println(1); 1 } }");
    let body = format!("{:?}", p.decls[0].kind);
    assert!(body.contains("\"println\""), "{body}");
}

#[test]
fn two_uses_of_one_name_collide() {
    let program = parse(
        lex("mod a { fn f() -> i64 { 1 } }
             mod b { fn f() -> i64 { 2 } }
             use a::f;
             use b::f;
             fn g() -> i64 { f() }")
        .unwrap(),
    )
    .unwrap();
    let errors = resolve_program(&program).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("two `use` declarations")), "{errors:?}");
}
