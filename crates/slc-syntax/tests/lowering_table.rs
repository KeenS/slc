//! Every row of the lowering table in the language design must correspond to
//! an actual lowering implementation.
//!
//! The table gives each row a construct id. This test reads those ids out of
//! `docs/design/` and requires a fixture for each: a program that uses the
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
    Row { id: "expr.literal", source: "func f() -> i32 { 42 }", core: "$int_42" },
    Row { id: "expr.ident", source: "func f(x: +i32) -> i32 { x }", core: "λx. x" },
    Row {
        id: "expr.enum",
        source: "enum Color { Red } func f() -> Color { Color::Red }",
        core: "Color::Red($unit)",
    },
    Row { id: "expr.call", source: "func f() -> i32 { g() }", core: "⟨g ∥ $unit · __call⟩" },
    Row {
        id: "expr.lambda",
        source: "func f() -> i32 { fn(x: +i32) -> i32 { x } }",
        core: "λx. x",
    },
    Row { id: "expr.pair", source: "func f() -> i32 { (1, 2) }", core: "($int_1 ⊗ $int_2)" },
    Row { id: "expr.inject", source: "func f() -> i64 { ::0(1) }", core: "|0($int_1)" },
    Row {
        id: "expr.flow",
        source: "func f(k: -i32) <- i32 { <1 | k> }",
        core: "μ__cut. ⟨$int_1 ∥ k⟩",
    },
    Row {
        id: "expr.let", source: "func f() -> i32 { let x = 1; x }", core: "μlet. ⟨$int_1 ∥ μ̃x."
    },
    Row { id: "expr.block", source: "func f() -> i32 { println(1); 2 }", core: "μ̃__discarded." },
    Row {
        id: "expr.mu",
        source: "func f() -> i32 { mu i32 { k <= <1 | k> } }",
        core: "μk. ⟨μ__cut. ⟨$int_1 ∥ k⟩ ∥ k⟩",
    },
    Row {
        id: "expr.match",
        source: "func f(x: +i32) -> i32 { of x { 1 => 1, _ => 0 } }",
        core: "__match_dispatch",
    },
    Row {
        id: "expr.data",
        source: "data S { a: i32 } func f() -> i32 { use_struct(S { a: 1 }) }",
        core: "S($int_1)",
    },
    Row {
        id: "expr.select",
        source: "enum Color { Red } func k(return: -i32) <- Color { mu Color { Red => <0 | return> } }",
        core: "co(μ̃[Color; Color::Red(). ⟨$int_0 ∥ return⟩])",
    },
    Row {
        id: "expr.consumer_argument",
        // A consumer is a value: it passes as an ordinary argument.
        source: "func f(k: -i64) <- i64 { <k | g }",
        core: "⟨g ∥ k · __call⟩",
    },
    Row {
        id: "expr.handler",
        source: "hook Reader { func read() -> i64; } func f() -> Handler<i64, i64, {Reader}, {}> { op Reader { read(): resume => <1 | resume } }",
        core: "__clauses",
    },
    Row {
        id: "expr.with_handler",
        source: "func f(reader: Handler<i64, i64, {}, {}>) -> i64 { op reader do 42 }",
        core: "__handle",
    },
    Row { id: "decl.fn.returning", source: "func f(x: +i32) -> i32 { x }", core: "λx. x" },
    Row { id: "decl.fn.transformer", source: "func f(k: -i32) <- i32 { <1 | k> }", core: "λk." },
    Row { id: "decl.mu", source: "proc f(x: +i32) | (k: -i32) { <x | k> }", core: "λx. λk." },
    Row { id: "decl.const", source: "def C: +i32 = 1;", core: "$int_1" },
    Row {
        id: "expr.request",
        source: "menu M { v: i32 } func f(k: -M) -> -M { of k { .v(out) <= .v(out) } }",
        core: "co(.M::v(out))",
    },
    Row {
        id: "decl.form",
        source: "form F { a: i32 } func g(k: -i32) -> F { mu F { F { a } => <a | k> } }",
        core: "co(μ̃[F; F(a). ⟨a ∥ k⟩])",
    },
    Row {
        id: "expr.comatch",
        source: "menu M { v: i32 } func g() -> M { mu { v: out <= <1 | out> } }",
        core: "μ[M; .M::v(out). ⟨$int_1 ∥ out⟩]",
    },
    Row {
        id: "decl.menu",
        source: "menu M { v: i32 } func g() -> M { mu M { v: out <= <1 | out> } }",
        core: "μ[M; .M::v(out). ⟨$int_1 ∥ out⟩]",
    },
    Row {
        id: "decl.enum",
        source: "enum Color { Red } func f() -> i32 { 0 }",
        core: "Color::Red($unit)",
    },
    Row { id: "decl.data", source: "data S { a: i32 } func f() -> i32 { 0 }", core: "$int_0" },
];

fn documented_ids() -> Vec<String> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut parts: Vec<_> = std::fs::read_dir(root.join("docs/design"))
        .expect("docs/design")
        .map(|entry| entry.expect("a design part").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect();
    parts.sort();
    let design = parts
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("a design part"))
        .find(|text| text.contains("### Lowering table"))
        .expect("the design has a lowering table");
    let table = design
        .split("### Lowering table")
        .nth(1)
        .expect("the lowering table has a body")
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
    assert!(!documented.is_empty(), "no lowering-table rows were found in the design");

    for id in &documented {
        let row = ROWS
            .iter()
            .find(|row| row.id == id)
            .unwrap_or_else(|| panic!("the design documents `{id}`, but no fixture covers it"));
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
    let printed = lowered("func show(out: -i64) <- +i64 { mu +i64 { n => <n | out> } }");
    assert!(
        printed.contains("co(μ̃n. ⟨n ∥ out⟩)"),
        "an atom's consumer should be a bare μ̃: {printed}"
    );

    // One binder more, and it is the product consumer instead.
    let printed =
        lowered("func show(out: -i64) <- (+i64, +i64) { mu (+i64, +i64) { (a, b) => <a | out> } }");
    assert!(printed.contains("co(μ̃(a, b)."), "a product keeps its binder list: {printed}");
}
