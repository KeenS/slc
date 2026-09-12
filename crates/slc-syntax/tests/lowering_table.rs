//! Every row of the lowering table in `DESIGN.md` must correspond to an
//! actual lowering implementation.
//!
//! The table gives each row a construct id. This test reads those ids out of
//! the document and requires a fixture for each: a program that uses the
//! construct, and the core shape its lowering is documented to produce. A row
//! that is added to the document without an implementation, or an id that is
//! renamed, fails here.

use slc_syntax::lexer::lex;
use slc_syntax::lower::lower_program;
use slc_syntax::parser::parse;

struct Row {
    id: &'static str,
    source: &'static str,
    /// A fragment of the printed core that only this lowering produces.
    core: &'static str,
}

const ROWS: &[Row] = &[
    Row { id: "expr.literal", source: "fn f() -> i32 { 42 }", core: "$int_42" },
    Row { id: "expr.ident", source: "fn f(x: +i32) -> i32 { x }", core: "λx. x" },
    Row {
        id: "expr.enum",
        source: "enum Color { Red } fn f() -> Color { Color::Red }",
        core: "Color::Red($unit)",
    },
    Row { id: "expr.call", source: "fn f() -> i32 { g(1) }", core: "⟨g ∥ $int_1 · __call⟩" },
    Row { id: "expr.lambda", source: "fn f() -> i32 { fn(x: +i32) -> i32 { x } }", core: "λx. x" },
    Row { id: "expr.pair", source: "fn f() -> i32 { (1, 2) }", core: "($int_1 ⊗ $int_2)" },
    Row {
        id: "expr.cut", source: "fn f(k: -i32) <- i32 { 1 @ k }", core: "μ__cut. ⟨$int_1 ∥ k⟩"
    },
    Row {
        id: "expr.let", source: "fn f() -> i32 { let x = 1; x }", core: "μlet. ⟨$int_1 ∥ μ̃x."
    },
    Row { id: "expr.block", source: "fn f() -> i32 { println(1); 2 }", core: "μ̃__discarded." },
    Row {
        id: "expr.if",
        source: "fn f() -> i32 { if true { 1 } else { 2 } }",
        core: "__if_dispatch",
    },
    Row {
        id: "expr.binop", source: "fn f() -> i32 { 1 + 2 }", core: "⟨add ∥ $int_1 · __call⟩"
    },
    Row { id: "expr.unop", source: "fn f() -> i32 { -1 }", core: "⟨neg ∥" },
    Row { id: "expr.index", source: "fn f(s: +String) -> char { s[0] }", core: "⟨__index ∥" },
    Row {
        id: "expr.slice",
        source: "fn f(s: +String) -> String { s[0..1] }",
        core: "⟨substring ∥",
    },
    Row {
        id: "expr.mu",
        source: "fn f() -> i32 { mu i32 { k <= k(1) } }",
        core: "μk. ⟨μ__call. ⟨k ∥ $int_1 · __call⟩ ∥ k⟩",
    },
    Row {
        id: "expr.match",
        source: "fn f(x: +i32) -> i32 { match x { 1 => 1, _ => 0 } }",
        core: "__match_dispatch",
    },
    Row {
        id: "expr.data",
        source: "data S { a: i32 } fn f() -> i32 { use_struct(S { a: 1 }) }",
        core: "S($int_1)",
    },
    Row {
        id: "expr.select",
        source: "enum Color { Red } fn k(return: -i32) <- Color { select Color { Red => 0 @ return } }",
        core: "co(μ̃[Color; Color::Red(). ⟨$int_0 ∥ return⟩])",
    },
    Row {
        id: "expr.shift",
        // The box erases: the core sees the consumer itself.
        source: "fn f(k: -i64) <- i64 { g(↓k) }",
        core: "⟨g ∥ k · __call⟩",
    },
    Row { id: "decl.fn.positive", source: "fn f(x: +i32) -> i32 { x }", core: "λx. x" },
    Row { id: "decl.fn.negative", source: "fn f(k: -i32) <- i32 { k(1) }", core: "λk." },
    Row { id: "decl.mu", source: "command f(x: +i32) | (k: -i32) { k(x) }", core: "λx. λk." },
    Row { id: "decl.const", source: "const C: +i32 = 1;", core: "$int_1" },
    Row {
        id: "expr.request",
        source: "menu M { v: i32 } fn f(k: -M) -> -M { match k { .v(out) <= .v(out) } }",
        core: "co(.M::v(out))",
    },
    Row {
        id: "decl.form",
        source: "form F { a: i32 } fn g(k: ↓-i32) -> F { select F { F { a } => a @ ↑k } }",
        core: "co(μ̃[F; F(a). ⟨a ∥ k⟩])",
    },
    Row {
        id: "expr.comatch",
        source: "menu M { v: i32 } fn g() -> M { mu { v: out <= 1 @ out } }",
        core: "μ[M; .M::v(out). ⟨$int_1 ∥ out⟩]",
    },
    Row {
        id: "decl.menu",
        source: "menu M { v: i32 } fn g() -> M { mu M { v: out <= 1 @ out } }",
        core: "μ[M; .M::v(out). ⟨$int_1 ∥ out⟩]",
    },
    Row {
        id: "decl.enum",
        source: "enum Color { Red } fn f() -> i32 { 0 }",
        core: "Color::Red($unit)",
    },
    Row { id: "decl.data", source: "data S { a: i32 } fn f() -> i32 { 0 }", core: "$int_0" },
];

fn documented_ids() -> Vec<String> {
    let design = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../DESIGN.md"))
        .expect("DESIGN.md");
    let table = design
        .split("### Lowering table")
        .nth(1)
        .expect("DESIGN.md has a lowering table")
        .split("### Surface-to-core coverage")
        .next()
        .expect("the lowering table ends before the coverage table");
    table
        .lines()
        .filter_map(|line| {
            let id = line.strip_prefix("| `")?.split('`').next()?;
            id.contains('.').then(|| id.to_string())
        })
        .collect()
}

fn lowered(source: &str) -> String {
    let program = parse(lex(source).expect("lex")).expect("parse");
    lower_program(&program)
        .expect("lower")
        .iter()
        .map(|(name, term)| format!("{name} = {term}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_documented_lowering_row_has_an_implementation() {
    let documented = documented_ids();
    assert!(!documented.is_empty(), "no lowering-table rows were found in DESIGN.md");

    for id in &documented {
        let row = ROWS
            .iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("DESIGN.md documents `{id}`, but no fixture covers it"));
        let printed = lowered(row.source);
        assert!(
            printed.contains(row.core),
            "`{id}` does not lower as documented.\nexpected to contain: {}\nactual:\n{printed}",
            row.core
        );
    }

    for row in ROWS {
        assert!(
            documented.iter().any(|id| id == row.id),
            "fixture `{}` covers a construct the lowering table does not document",
            row.id
        );
    }
}

#[test]
fn select_over_an_atom_is_the_value_abstraction() {
    // A positive type with one shape and one component is the degenerate
    // product: its consumer has one arm, whose one binder takes the whole
    // value. That is `μ̃x. c` — the same co-term every binder lowers to, here
    // written directly instead of being generated by `let`.
    let printed = lowered("fn show(out: -i64) <- +i64 { select +i64 { n => n @ out } }");
    assert!(
        printed.contains("co(μ̃n. ⟨n ∥ out⟩)"),
        "an atom's consumer should be a bare μ̃: {printed}"
    );

    // One binder more, and it is the product consumer instead.
    let printed = lowered(
        "fn show(out: -i64) <- (+i64 ⊗ +i64) { select (+i64 ⊗ +i64) { (a, b) => a @ out } }",
    );
    assert!(printed.contains("co(μ̃(a, b)."), "a product keeps its binder list: {printed}");
}
