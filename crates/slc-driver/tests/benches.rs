//! The integers printed by `benches/`. Timing is `benches/run.sh`.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn checksums(root: &std::path::Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(root.join("benches/checksums.txt")).expect("checksums.txt");
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap_or_else(|| panic!("bad checksums line: {line}"));
        let expected = parts.next().unwrap_or_else(|| panic!("bad checksums line: {line}"));
        assert!(parts.next().is_none(), "bad checksums line: {line}");
        rows.push((name.to_string(), expected.to_string()));
    }
    assert!(!rows.is_empty(), "benches/checksums.txt is empty");
    rows
}

#[test]
fn benchmark_programs_print_their_checksums() {
    let root = repo_root();
    let rows = checksums(&root);
    let mut listed: Vec<String> = rows.iter().map(|(name, _)| name.clone()).collect();
    let mut on_disk = Vec::new();
    for entry in std::fs::read_dir(root.join("benches")).expect("benches/") {
        let path = entry.expect("a benches entry").path();
        if path.extension().is_some_and(|ext| ext == "sl") {
            on_disk.push(path.file_stem().unwrap().to_string_lossy().into_owned());
        }
    }
    listed.sort();
    on_disk.sort();
    assert_eq!(on_disk, listed, "benches/*.sl and checksums.txt disagree");

    let mut failed = Vec::new();
    for (name, expected) in &rows {
        let file = root.join(format!("benches/{name}.sl"));
        let out = Command::new(env!("CARGO_BIN_EXE_slc"))
            .args(["run", file.to_str().unwrap()])
            .current_dir(&root)
            .output()
            .expect("failed to run slc");
        let got = String::from_utf8(out.stdout).expect("utf-8 stdout");
        let got = got.trim();
        if !out.status.success() || got != expected {
            failed.push(format!(
                "{name}: expected {expected}, got {got:?}, stderr {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}
