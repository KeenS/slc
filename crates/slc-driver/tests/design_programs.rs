//! Complete programs in the language design compile.
//!
//! A code block in `DESIGN.md` or a part under `docs/design/` is either a
//! fragment, which marks what it leaves out with `…`, or a complete program,
//! which declares `main` and leaves nothing out. Every complete program is
//! run here and must get past every check; what it does once it runs — a
//! file it reads may not exist — is its own business.

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
                if text.contains("proc main") && !text.contains('…') {
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

/// `DESIGN.md` and every markdown part under `docs/design/`, in path order.
fn design_sources(root: &std::path::Path) -> Vec<(String, String)> {
    let mut paths = vec![root.join("DESIGN.md")];
    let mut parts: Vec<PathBuf> = std::fs::read_dir(root.join("docs/design"))
        .expect("docs/design")
        .map(|entry| entry.expect("a design part").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect();
    parts.sort();
    paths.extend(parts);
    paths
        .into_iter()
        .map(|path| {
            let label = path.strip_prefix(root).unwrap_or(&path).display().to_string();
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{label}: {error}"));
            (label, text)
        })
        .collect()
}

#[test]
fn design_programs_compile() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut programs = Vec::new();
    for (label, design) in design_sources(&root) {
        for (line, program) in complete_programs(&design) {
            programs.push((label.clone(), line, program));
        }
    }
    assert!(!programs.is_empty(), "the design has no complete program to check");
    for (label, line, program) in programs {
        let path = std::env::temp_dir()
            .join(format!("slc_design_program_{}_{line}.sl", label.replace('/', "_")));
        std::fs::write(&path, &program).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_slc"))
            .args(["run", path.to_str().unwrap()])
            .current_dir(&root)
            .output()
            .expect("failed to run slc");
        let stderr = String::from_utf8_lossy(&out.stderr);
        if let Some(stage) = COMPILE_ERRORS.iter().find(|stage| stderr.contains(*stage)) {
            panic!("{label}:{line}: the program does not compile ({stage}):\n{stderr}");
        }
        if !out.status.success() {
            let program_error = stderr.contains("error: type mismatch:")
                || stderr.contains("evaluation diverged (fuel exhausted)")
                || stderr.contains("a continuation left the handler it was captured under");
            let exited = stderr.is_empty() && out.status.code().is_some();
            if !program_error && !exited {
                panic!(
                    "{label}:{line}: status {:?} is a compiler or link diagnostic:\n{stderr}",
                    out.status.code()
                );
            }
        }
    }
}
