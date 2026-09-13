//! `DESIGN.md`'s complete programs compile.
//!
//! A code block in the document is either a fragment, which marks what it
//! leaves out with `…`, or a complete program, which declares `main` and
//! leaves nothing out. Every complete program is run here and must get past
//! every check; what it does once it runs — a file it reads may not exist —
//! is its own business.

use std::path::PathBuf;
use std::process::Command;

/// What the driver writes before an error found ahead of running.
const COMPILE_ERRORS: &[&str] = &[
    "parse error: ",
    "resolve: ",
    "trait: ",
    "type: ",
    "polarity: ",
    "exhaustiveness: ",
    "effect: ",
    "lowering: ",
    "entry point must be",
    "no `main`",
];

/// The `sl` blocks that are complete programs, each with the line its fence
/// opens on.
fn complete_programs(design: &str) -> Vec<(usize, String)> {
    let mut programs = Vec::new();
    let mut block: Option<(usize, String)> = None;
    for (index, line) in design.lines().enumerate() {
        if let Some((start, text)) = block.as_mut() {
            if line.starts_with("```") {
                if text.contains("command main") && !text.contains('…') {
                    programs.push((*start, std::mem::take(text)));
                }
                block = None;
            } else {
                text.push_str(line);
                text.push('\n');
            }
        } else if line.starts_with("```sl") {
            block = Some((index + 1, String::new()));
        }
    }
    programs
}

#[test]
fn design_programs_compile() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let design = std::fs::read_to_string(root.join("DESIGN.md")).expect("DESIGN.md");
    let programs = complete_programs(&design);
    assert!(!programs.is_empty(), "DESIGN.md has no complete program to check");
    for (line, program) in programs {
        let path = std::env::temp_dir().join(format!("slc_design_program_{line}.sl"));
        std::fs::write(&path, &program).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_slc"))
            .args(["run", path.to_str().unwrap()])
            .current_dir(&root)
            .output()
            .expect("failed to run slc");
        let stderr = String::from_utf8_lossy(&out.stderr);
        if let Some(stage) = COMPILE_ERRORS.iter().find(|stage| stderr.contains(*stage)) {
            panic!("DESIGN.md:{line}: the program does not compile ({stage}):\n{stderr}");
        }
    }
}
