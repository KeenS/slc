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
             func f() -> i64 { 1 }
             mod b { func g() -> i64 { 2 } }
         }
         func h() -> i64 { 3 }",
    );
    assert_eq!(names(&p), vec!["a::f", "a::b::g", "h"]);
}

#[test]
fn references_resolve_by_scope() {
    // In-module bare names, sibling paths, and the walk to an ancestor.
    let p = resolved(
        "mod outer {
             func shared() -> i64 { 1 }
             mod inner { pub func deep() -> i64 { shared() } }
             func from_sibling() -> i64 { inner::deep() }
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
                lex("mod m { func helper() -> i64 { 1 }
                           pub func f() -> i64 { helper() }
                           mod deeper { pub func g() -> i64 { helper() } } }")
                .unwrap()
            )
            .unwrap()
        )
        .is_ok()
    );
    // And nowhere else.
    let errors = resolve_program(
        &parse(
            lex("mod m { func helper() -> i64 { 1 } }
                    func caller() -> i64 { m::helper() }")
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
        "mod m { pub func f() -> i64 { 1 } }
         use m::f;
         func caller() -> i64 { f() }
         func shadows() -> i64 { let f = 5; f }",
    );
    let caller = format!("{:?}", p.decls[1].kind);
    assert!(caller.contains("\"m::f\""), "{caller}");
    let shadows = format!("{:?}", p.decls[2].kind);
    assert!(!shadows.contains("m::f"), "a local binding shadows the use: {shadows}");
}

#[test]
fn an_unclaimed_name_is_left_for_later_passes() {
    // `str_len` is a builtin: no module claims it, so it stays bare.
    let p = resolved("mod m { func f() -> i64 { str_len(\"x\"); 1 } }");
    let body = format!("{:?}", p.decls[0].kind);
    assert!(body.contains("\"str_len\""), "{body}");
}

#[test]
fn two_uses_of_one_name_collide() {
    let program = parse(
        lex("mod a { func f() -> i64 { 1 } }
             mod b { func f() -> i64 { 2 } }
             use a::f;
             use b::f;
             func g() -> i64 { f() }")
        .unwrap(),
    )
    .unwrap();
    let errors = resolve_program(&program).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("two `use` declarations")), "{errors:?}");
}

/// The body of the declaration named `name`, as resolved.
fn body_of(program: &slc_syntax::ast::Program, name: &str) -> String {
    program
        .decls
        .iter()
        .find(|d| matches!(&d.kind, Decl::Fn { name: n, .. } if n == name))
        .map(|d| format!("{:?}", d.kind))
        .unwrap_or_default()
}

#[test]
fn a_module_glob_brings_every_pub_member_and_nothing_private() {
    let p = resolved(
        "mod m { pub func f() -> i64 { 1 } func g() -> i64 { 2 } }
         use m::*;
         func uses_f() -> i64 { f() }
         func uses_g() -> i64 { g() }",
    );
    assert!(body_of(&p, "uses_f").contains("\"m::f\""), "{:?}", body_of(&p, "uses_f"));
    assert!(!body_of(&p, "uses_g").contains("m::g"), "a glob must not reach a private member");
}

#[test]
fn a_named_use_and_a_declaration_beat_a_glob() {
    let p = resolved(
        "mod a { pub func f() -> i64 { 1 } pub func h() -> i64 { 3 } }
         mod b { pub func f() -> i64 { 2 } }
         use a::*;
         use b::f;
         func h() -> i64 { 4 }
         func by_name() -> i64 { f() }
         func declared() -> i64 { h() }",
    );
    assert!(body_of(&p, "by_name").contains("\"b::f\""), "{:?}", body_of(&p, "by_name"));
    assert!(!body_of(&p, "declared").contains("a::h"), "{:?}", body_of(&p, "declared"));
}

#[test]
fn two_globs_may_share_a_name_until_it_is_used() {
    let two = "mod a { pub func f() -> i64 { 1 } } mod b { pub func f() -> i64 { 2 } }
               use a::*; use b::*;";
    assert!(
        resolve_program(
            &parse(lex(&format!("{two} func quiet() -> i64 {{ 0 }}")).unwrap()).unwrap()
        )
        .is_ok()
    );
    let errors = resolve_program(
        &parse(lex(&format!("{two} func loud() -> i64 {{ f() }}")).unwrap()).unwrap(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("more than one glob")
            && e.message.contains("`a::f`")
            && e.message.contains("`b::f`")),
        "{errors:?}"
    );
}
