//! `spawn` runs a proc with `main`'s signature on another host thread.
//!
//! The call returns `(,)` before the proc finishes. The process waits for
//! it, and the `i32` passed to `exit` is not the process status. A fatal in
//! the proc fails the process. The native runtime is x86-64 Linux.

use std::process::Command;

fn run(interpret: bool, name: &str, source: &str) -> (String, String, bool) {
    let path = std::env::temp_dir().join(format!("slc_spawn_{}_{name}.sl", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_slc"));
    cmd.arg("run");
    if interpret {
        cmd.arg("--interpret");
    }
    cmd.arg(path.to_str().unwrap());
    let out = cmd.output().expect("slc");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn assert_both(name: &str, source: &str, stdout: &str) {
    let (out, err, ok) = run(true, name, source);
    assert!(ok, "interpret\n{err}");
    assert_eq!(out, stdout, "interpret\n{err}");
    assert!(err.is_empty(), "interpret\n{err}");
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let (out, err, ok) = run(false, name, source);
        assert!(ok, "native\n{err}");
        assert_eq!(out, stdout, "native\n{err}");
        assert!(err.is_empty(), "native\n{err}");
    }
}

const WORKER: &str = "\
proc worker | (exit: i32) / {IO} {
    <\"from worker\" | println;
    <7 | exit>
}
proc main | (exit: i32) / {IO} {
    <worker | spawn;
    <0 | exit>
}
";

#[test]
fn the_proc_runs_and_its_status_is_not_the_process_status() {
    assert_both("status", WORKER, "from worker\n");
}

const OVERLAP: &str = "\
proc worker | (exit: i32) / {IO} {
    <200000000 | __sleep_ns;
    <\"child\" | println;
    <0 | exit>
}
proc main | (exit: i32) / {IO} {
    <worker | spawn;
    <\"parent\" | println;
    <0 | exit>
}
";

#[test]
fn the_parent_continues_and_the_proc_still_finishes() {
    assert_both("overlap", OVERLAP, "parent\nchild\n");
}

const BROKEN: &str = "\
proc worker | (exit: i32) / {IO} {
    let n = <(1, 0) | div;
    <n | println;
    <0 | exit>
}
proc main | (exit: i32) / {IO} {
    <worker | spawn;
    <0 | exit>
}
";

#[test]
fn a_fatal_in_the_proc_fails_the_process() {
    let (stdout, stderr, ok) = run(true, "fatal", BROKEN);
    assert!(!ok, "interpret succeeded\n{stdout}{stderr}");
    assert!(stderr.contains("error: type mismatch: division by zero"), "interpret\n{stderr}");
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let (stdout, stderr, ok) = run(false, "fatal", BROKEN);
        assert!(!ok, "native succeeded\n{stdout}{stderr}");
        assert!(stderr.contains("error: type mismatch: division by zero"), "native\n{stderr}");
    }
}

#[test]
fn a_returning_func_is_not_a_proc() {
    let source = "\
func worker() -> i32 / {IO} {
    <\"no\" | println;
    1
}
proc main | (exit: i32) / {IO} {
    <worker | spawn;
    <0 | exit>
}
";
    let path = std::env::temp_dir().join(format!("slc_spawn_{}_check.sl", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let out =
        Command::new(env!("CARGO_BIN_EXE_slc")).arg("check").arg(&path).output().expect("slc");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("spawn"), "{stderr}");
}
