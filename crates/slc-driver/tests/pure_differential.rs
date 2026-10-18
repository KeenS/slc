#![cfg(all(target_arch = "x86_64", target_os = "linux"))]
//! ELF versus `slc run --interpret` on the pure examples. `slc run` links an ELF.

use std::path::{Path, PathBuf};
use std::process::Command;

const PRELUDE: &str = include_str!("../src/prelude.sl");

/// Same clamp as `main.rs`: `try_into::<u8>`, and any other `i32` is status 1.
fn driver() -> String {
    format!(
        r#"
#include <stdint.h>
{externs}
int main(void) {{
    uint64_t status = {call};
    int32_t code = (int32_t)status;
    if (code < 0 || code > 255) return 1;
    return (int)code;
}}
"#,
        externs = slc_native::entry_externs(),
        call = slc_native::entry_call("~(uint64_t)0"),
    )
}

fn compile_src(src: &str) -> Vec<u8> {
    let src = format!("{src}\n{PRELUDE}");
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let tokens = slc_syntax::lexer::lex(&src).unwrap_or_else(|err| panic!("{err:?}"));
            let program = slc_syntax::parser::parse(tokens).unwrap_or_else(|err| panic!("{err:?}"));
            let program = slc_syntax::resolve::resolve_program(&program)
                .unwrap_or_else(|err| panic!("{err:?}"));
            let (program, traits) =
                slc_syntax::traits::elaborate(&program).unwrap_or_else(|err| panic!("{err:?}"));
            let (dispatch, rows) = slc_check::expr::check_program_with_rows(&program, &traits)
                .unwrap_or_else(|diags| panic!("{diags:?}"));
            assert!(rows.is_empty(), "{rows:?}");
            slc_check::polarity::check_program(&program)
                .unwrap_or_else(|diags| panic!("{diags:?}"));
            slc_check::exhaustive::check_exhaustiveness(&program)
                .unwrap_or_else(|diags| panic!("{diags:?}"));
            let defs = slc_syntax::lower::lower_program_resolving(&program, &dispatch)
                .unwrap_or_else(|err| panic!("{err}"));
            let operations = program.operation_names();
            slc_native::compile(
                &defs,
                &dispatch.specializations,
                &dispatch.payloads,
                &traits,
                &operations,
                usize::MAX,
            )
            .unwrap_or_else(|err| panic!("{err}"))
            .object
        })
        .expect("compile thread")
        .join()
        .unwrap_or_else(|err| panic!("compile panicked: {err:?}"))
}

fn interpret(path: &Path) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_slc"))
        .args(["run", "--interpret", path.to_str().unwrap()])
        .output()
        .expect("slc run");
    let code = out.status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        128 + out.status.signal().unwrap_or(0)
    });
    (
        code,
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn link_run(object: &[u8]) -> (i32, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "slc-pure-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.o"), object).unwrap();
    std::fs::write(dir.join("main.c"), driver()).unwrap();
    let exe = dir.join("p");
    slc_native::link_pie(&exe, &dir.join("main.c"), &dir.join("p.o"))
        .unwrap_or_else(|error| panic!("link failed\n{error}"));
    let ran = Command::new(&exe).output().unwrap();
    let code = ran.status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        128 + ran.status.signal().unwrap_or(0)
    });
    (
        code,
        String::from_utf8_lossy(&ran.stdout).into_owned(),
        String::from_utf8_lossy(&ran.stderr).into_owned(),
    )
}

fn compare(name: &str, source: &str, path: &Path, needle: Option<&str>) {
    let native = link_run(&compile_src(source));
    let interp = interpret(path);
    assert_eq!(native, interp, "{name}");
    if let Some(needle) = needle {
        assert!(native.2.contains(needle), "{name}\n{}", native.2);
    }
}

fn scratch(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slc-pure-src-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.sl"));
    std::fs::write(&path, source).unwrap();
    path
}

#[test]
fn pure_primitives_match_the_interpreter() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["hello.sl", "arithmetic.sl", "into.sl", "primitive_widths.sl"] {
        let path = root.join("examples/basics").join(name);
        let source = std::fs::read_to_string(&path).unwrap();
        compare(name, &source, &path, None);
    }

    let zero = "\
proc main | (exit: i32) / {IO} {
    let text = of 0 { 0 => \"zero\", _ => \"other\" };
    <text | println;
    <0 | exit>
}
";
    compare("literal zero", zero, &scratch("zero", zero), None);

    let other = "\
proc main | (exit: i32) / {IO} {
    let text = of 1 { 0 => \"zero\", _ => \"other\" };
    <text | println;
    <0 | exit>
}
";
    compare("literal one", other, &scratch("one", other), None);

    let overflow = "\
proc main | (exit: i32) / {IO} {
    <(9223372036854775807, 1) | add | println;
    <0 | exit>
}
";
    compare("overflow", overflow, &scratch("overflow", overflow), Some("arithmetic overflow:"));

    let div0 = "\
proc main | (exit: i32) / {IO} {
    <(1, 0) | div | println;
    <0 | exit>
}
";
    compare("division by zero", div0, &scratch("div0", div0), Some("division by zero"));

    let frem = "\
proc main | (exit: i32) / {IO} {
    <(-0.0, 1.0) | rem | println;
    let inf = <(1.0, 0.0) | div;
    <(1.0, inf) | rem | println;
    <0 | exit>
}
";
    compare("float rem", frem, &scratch("frem", frem), None);

    let missed = "\
proc main | (exit: i32) / {IO} {
    <(\"hi\", 3) | index | println;
    <0 | exit>
}
";
    compare(
        "index",
        missed,
        &scratch("index", missed),
        Some("builtin type mismatch: index 3 out of range"),
    );

    // `high` is written before `low`, and the middle method is the default.
    // The dictionary still projects the trait's index. Names stay off `Ord`.
    let flipped = "\
spec Flip {
    func low(self: Self, other: Self) -> i64;
    func mid(self: Self, other: Self) -> i64 { 7 }
    func high(self: Self, other: Self) -> i64;
}
impl Flip for i64 {
    func high(self: i64, other: i64) -> i64 { 3 }
    func low(self: i64, other: i64) -> i64 { 1 }
}
func call_low<+T: Flip>(a: T, b: T) -> i64 { <(a, b) | low }
func call_mid<+T: Flip>(a: T, b: T) -> i64 { <(a, b) | mid }
func call_high<+T: Flip>(a: T, b: T) -> i64 { <(a, b) | high }
proc main | (exit: i32) / {IO} {
    <(<(1, 2) | call_low) | println;
    <(<(1, 2) | call_mid) | println;
    <(<(1, 2) | call_high) | println;
    <0 | exit>
}
";
    compare("dict order", flipped, &scratch("flip", flipped), None);

    let reals = "\
func widen(n: i64) -> f64 { <n | into }
func whole(n: f64) -> i64 { <n | into }
proc main | (exit: i32) / {IO} {
    <4.0 | sqrt | println;
    <-1.5 | floor | println;
    <4 | widen | println;
    <4.0 | whole | println;
    <0 | exit>
}
";
    compare("reals", reals, &scratch("reals", reals), None);

    let inexact = "\
func widen(n: i64) -> f64 { <n | into }
proc main | (exit: i32) / {IO} {
    <9223372036854775807 | widen | println;
    <0 | exit>
}
";
    compare(
        "inexact f64",
        inexact,
        &scratch("inexact", inexact),
        Some("9223372036854775807 does not fit in f64"),
    );

    let fraction = "\
func whole(n: f64) -> i64 { <n | into }
proc main | (exit: i32) / {IO} {
    <1.5 | whole | println;
    <0 | exit>
}
";
    compare("fraction", fraction, &scratch("fraction", fraction), Some("1.5 does not fit in i64"));

    let root = "\
proc main | (exit: i32) / {IO} {
    <-1.0 | sqrt | println;
    <0 | exit>
}
";
    compare("sqrt domain", root, &scratch("sqrt", root), Some("arithmetic overflow: sqrt(-1)"));
}
